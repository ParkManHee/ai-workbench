// 폰 작업별 격리 트리 — 폰에서 보낸 작업과 PC 터미널 세션이 같은 작업 트리를 동시에 고쳐 충돌하던 것.
// 원래 트리 옆에 `<저장소>.wt-phone-<id>`(브랜치 claude/phone-<id>)를 만들어 거기서 돌린다.
// 트랜스크립트 폴더는 cwd 마다 다르다(~/.claude/projects/<slug>) — 세션 목록·대화 읽기·이어 보내기가
// 원래 프로젝트 이름으로 이 세션을 찾도록 트리·세션 매핑을 여기 한 곳에 둔다.
use crate::routes::AppState;
use crate::sessions::{filter_rows, SessionRow};
use awb_core::transcript::{fresh_live_sessions, list_open_sessions_in, project_slug, transcript_path};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::process::Command;
use std::sync::Mutex;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Worktree {
    pub project: String,
    /// 원래 트리
    pub repo: String,
    pub path: String,
    pub branch: String,
    /// 만들 때의 HEAD — 지울 때 새 커밋이 있는지 세는 기준
    pub base: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub removed: bool,
}

#[derive(Serialize, Deserialize, Default)]
struct Map {
    /// 트리 경로 → 트리. 세션 ID 를 알기 전(첫 실행 중)에도 목록·프로젝트 필터가 이 트리를 안다
    #[serde(default)]
    trees: HashMap<String, Worktree>,
    /// 세션 ID → 트리 경로
    #[serde(default)]
    sessions: HashMap<String, String>,
}

pub struct WorktreeStore { path: String }

impl WorktreeStore {
    /// 세션 저장소 디렉터리에 둔다 — SessionStore::path 는 '.' 을 '_' 로 바꿔 `.worktrees.json` 과 겹치지 않는다
    pub fn at(dir: &str) -> WorktreeStore { WorktreeStore { path: format!("{dir}/.worktrees.json") } }
    fn read(&self) -> Map {
        std::fs::read_to_string(&self.path).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }
    fn update(&self, f: impl FnOnce(&mut Map)) -> std::io::Result<()> {
        // 읽고-고쳐-쓰기 사이에 다른 요청이 끼면 한쪽 매핑이 사라진다
        static LOCK: Mutex<()> = Mutex::new(());
        let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut m = self.read();
        f(&mut m);
        let tmp = format!("{}.tmp", self.path);
        std::fs::write(&tmp, serde_json::to_string(&m)?)?;
        std::fs::rename(&tmp, &self.path)
    }
    pub fn add(&self, wt: &Worktree) -> std::io::Result<()> {
        self.update(|m| { m.trees.insert(wt.path.clone(), wt.clone()); })
    }
    pub fn note_session(&self, sid: &str, path: &str) -> std::io::Result<()> {
        self.update(|m| { m.sessions.insert(sid.to_string(), path.to_string()); })
    }
    pub fn mark_removed(&self, path: &str) -> std::io::Result<()> {
        self.update(|m| { if let Some(w) = m.trees.get_mut(path) { w.removed = true; } })
    }
    pub fn for_session(&self, sid: &str) -> Option<Worktree> {
        let m = self.read();
        m.sessions.get(sid).and_then(|p| m.trees.get(p)).cloned()
    }
    /// 지운 트리도 돌려준다 — 트랜스크립트는 남아 있어 지난 대화로 보인다
    pub fn for_project(&self, project: &str) -> Vec<Worktree> {
        let mut v: Vec<Worktree> = self.read().trees.into_values().filter(|w| w.project == project).collect();
        v.sort_by(|a, b| a.path.cmp(&b.path));
        v
    }
    pub fn paths(&self) -> HashSet<String> { self.read().trees.into_keys().collect() }
}

pub fn store(st: &AppState) -> WorktreeStore { WorktreeStore::at(&st.sessions.dir) }

fn git(dir: &str, args: &[&str]) -> Result<String, String> {
    let o = Command::new("git").arg("-C").arg(dir).args(args).output().map_err(|e| format!("git 실행 실패: {e}"))?;
    if o.status.success() {
        Ok(String::from_utf8_lossy(&o.stdout).trim().to_string())
    } else {
        Err(String::from_utf8_lossy(&o.stderr).trim().to_string())
    }
}

/// 원래 트리의 현재 HEAD 에서 `../<저장소>.wt-phone-<id>` 를 만든다. 원래 트리의 브랜치는 건드리지 않는다.
/// 같은 이름의 폴더·브랜치가 있으면 git 이 거부한 그대로 돌려준다.
pub fn create(project: &str, repo: &str) -> Result<Worktree, String> {
    let id = &crate::pairing::random_token()[..6];
    let rp = std::path::Path::new(repo);
    let (Some(parent), Some(name)) = (rp.parent(), rp.file_name()) else { return Err(format!("트리 위치를 정할 수 없습니다: {repo}")) };
    let path = parent.join(format!("{}.wt-phone-{id}", name.to_string_lossy())).to_string_lossy().to_string();
    let branch = format!("claude/phone-{id}");
    let base = git(repo, &["rev-parse", "HEAD"])?;
    git(repo, &["worktree", "add", &path, "-b", &branch, &base])?;
    Ok(Worktree { project: project.to_string(), repo: repo.to_string(), path, branch, base, removed: false })
}

#[derive(Debug, PartialEq)]
pub enum Cleanup {
    Removed { branch_deleted: bool, note: Option<String> },
    Kept(String),
}

/// 변경도 새 커밋도 없을 때만 트리를 지우고 브랜치를 지운다. 강제 옵션(--force·-D)은 쓰지 않는다.
pub fn cleanup(wt: &Worktree) -> Cleanup {
    if awb_core::lock::held(&wt.path) { return Cleanup::Kept("이 트리에서 실행 중".into()); }
    let dirty = match git(&wt.path, &["status", "--porcelain"]) {
        Ok(s) => s.lines().filter(|l| !l.is_empty()).count(),
        Err(e) => return Cleanup::Kept(format!("상태 확인 실패: {e}")),
    };
    if dirty > 0 { return Cleanup::Kept(format!("커밋 안 한 변경 {dirty}개")); }
    let ahead = match git(&wt.path, &["rev-list", "--count", &format!("{}..HEAD", wt.base)]).map(|s| s.parse::<u64>()) {
        Ok(Ok(n)) => n,
        Ok(Err(e)) => return Cleanup::Kept(format!("커밋 수 확인 실패: {e}")),
        Err(e) => return Cleanup::Kept(format!("커밋 수 확인 실패: {e}")),
    };
    if ahead > 0 { return Cleanup::Kept(format!("새 커밋 {ahead}개 — 브랜치 {}", wt.branch)); }
    if let Err(e) = git(&wt.repo, &["worktree", "remove", &wt.path]) { return Cleanup::Kept(format!("트리 지우기 실패: {e}")); }
    match git(&wt.repo, &["branch", "-d", &wt.branch]) {
        Ok(_) => Cleanup::Removed { branch_deleted: true, note: None },
        Err(e) => Cleanup::Removed { branch_deleted: false, note: Some(format!("브랜치는 남김: {e}")) },
    }
}

/// 끝내기·보관 때 정리하고 응답에 실을 결과를 만든다. 지웠으면 매핑에 표시해 이어 보내기를 막는다.
pub fn cleanup_json(st: &AppState, wt: &Worktree) -> serde_json::Value {
    let r = cleanup(wt);
    if matches!(r, Cleanup::Removed { .. }) {
        if let Err(e) = store(st).mark_removed(&wt.path) { eprintln!("격리 트리 매핑 갱신 실패({}): {e}", wt.path); }
    }
    let mut v = serde_json::json!({ "path": wt.path, "branch": wt.branch });
    match r {
        Cleanup::Removed { branch_deleted, note } => { v["removed"] = true.into(); v["branch_deleted"] = branch_deleted.into(); if let Some(n) = note { v["reason"] = n.into(); } }
        Cleanup::Kept(reason) => { v["removed"] = false.into(); v["reason"] = reason.into(); }
    }
    v
}

/// 이어 보낼 세션이 격리 세션이면 그 트리에서 돌아야 한다. 지운 트리면 이어갈 수 없다.
pub fn for_resume(st: &AppState, project: &str, resume: Option<&str>) -> Result<Option<Worktree>, String> {
    let Some(wt) = resume.and_then(|s| store(st).for_session(s)).filter(|w| w.project == project) else { return Ok(None) };
    if wt.removed { return Err(format!("격리 트리({})를 지워서 이 대화는 이어갈 수 없습니다 — 새 대화로 시작하세요", wt.branch)); }
    Ok(Some(wt))
}

/// 실행이 잡은 세션 ID 를 그 실행의 격리 트리에 묶는다(격리 실행이 아니면 아무것도 안 한다).
pub fn note(st: &AppState, meta: &crate::runreg::RunMeta, sid: &str) {
    if let Some(wt) = &meta.worktree {
        if let Err(e) = store(st).note_session(sid, &wt.path) { eprintln!("격리 세션 매핑 저장 실패({sid}): {e}"); }
    }
}

/// 프로젝트의 세션 목록 — 원래 트리와 이 프로젝트 격리 트리들의 트랜스크립트 폴더를 합친다.
/// /sessions·/projects·/needs-you 가 모두 이것을 쓴다.
pub fn project_sessions(st: &AppState, project: &str, repo: &str, tx_root: &str, live_ids: &HashSet<String>, q: Option<&str>, archived: bool) -> Vec<SessionRow> {
    // 대화 파일이 아직 없는 새 터미널 세션도 행으로 — 폰에서 첫 메시지를 보낼 수 있게
    let reg = awb_core::live::registry();
    let open = |dir: &str| { let mut l = list_open_sessions_in(&format!("{tx_root}/{}", project_slug(dir)), live_ids); let f = fresh_live_sessions(&reg, live_ids, dir, &l); l.extend(f); l };
    let mut list = open(repo);
    let mut owner: HashMap<String, Worktree> = HashMap::new();
    for wt in store(st).for_project(project) {
        for s in open(&wt.path) {
            owner.insert(s.session_id.clone(), wt.clone());
            list.push(s);
        }
    }
    list.sort_by(|a, b| b.updated.cmp(&a.updated));
    let mut rows = filter_rows(list, &st.sessions.metas(project), q, archived);
    for r in &mut rows {
        r.worktree = owner.remove(&r.info.session_id);
        r.channel = r.info.live && awb_core::live::find_live(&r.info.session_id).is_some_and(|e| crate::channel::attached(e.pid));
    }
    rows
}

/// 대화 파일 — 원래 트리 폴더에 없으면 이 프로젝트 격리 트리들의 폴더에서 찾는다.
pub fn transcript_file(st: &AppState, project: &str, repo: &str, sid: &str) -> String {
    let first = transcript_path(&project_slug(repo), sid);
    if std::path::Path::new(&first).exists() { return first; }
    store(st).for_project(project).iter().map(|w| transcript_path(&project_slug(&w.path), sid))
        .find(|p| std::path::Path::new(p).exists()).unwrap_or(first)
}
