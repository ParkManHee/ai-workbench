// crates/awb-server/src/gitdiff.rs (Task 5) — git status/diff 파싱하여 변경 요약 생성 + 커밋·푸시·PR
use serde::Serialize;
use std::process::Command;

#[derive(Serialize, Clone)]
pub struct DiffEntry { pub path: String, pub status: String }

#[derive(Serialize, Clone)]
pub struct DiffSummary { pub files: u32, pub insertions: u32, pub deletions: u32, pub entries: Vec<DiffEntry> }

pub fn summarize(workdir: &str) -> DiffSummary {
    let status = Command::new("git").args(["-C", workdir, "status", "--porcelain"]).output();
    let entries: Vec<DiffEntry> = status.map(|o| {
        String::from_utf8_lossy(&o.stdout).lines().filter(|l| !l.trim().is_empty()).map(|l| {
            // porcelain v1 형식: "XY <path>" — 상태 접두 2글자는 항상 ASCII(단일바이트)이므로
            // 바이트 인덱스 2에서 잘라도 char-boundary 문제가 없다(경로에 멀티바이트 문자가 있어도 안전).
            let (st, path) = if l.len() >= 2 { l.split_at(2) } else { l.split_at(0) };
            DiffEntry { path: path.trim().to_string(), status: st.trim().to_string() }
        }).collect()
    }).unwrap_or_default();
    let numstat = Command::new("git").args(["-C", workdir, "diff", "--numstat"]).output();
    let (mut ins, mut del) = (0u32, 0u32);
    if let Ok(o) = numstat {
        for l in String::from_utf8_lossy(&o.stdout).lines() {
            let mut it = l.split_whitespace();
            if let (Some(a), Some(b)) = (it.next(), it.next()) {
                ins += a.parse::<u32>().unwrap_or(0);
                del += b.parse::<u32>().unwrap_or(0);
            }
        }
    }
    DiffSummary { files: entries.len() as u32, insertions: ins, deletions: del, entries }
}

/// 파일 하나의 unified diff — 워킹트리 → 스테이징 → 미추적(/dev/null 대비) 순으로 시도.
/// 폰 화면용이므로 20k자에서 절단한다.
pub fn file_diff(workdir: &str, file: &str) -> String {
    // 경로 인젝션 방어: 절대경로·상위 탈출 금지(요약 entries의 상대경로만 유효)
    if file.starts_with('/') || file.split('/').any(|c| c == "..") { return String::new(); }
    let mut d = run(&["-C", workdir, "diff", "--", file]);
    if d.trim().is_empty() { d = run(&["-C", workdir, "diff", "--cached", "--", file]); }
    if d.trim().is_empty() { d = run(&["-C", workdir, "diff", "--no-index", "--", "/dev/null", file]); }
    cap(d)
}

fn run(args: &[&str]) -> String {
    Command::new("git").args(args).output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default()
}

fn cap(d: String) -> String {
    if d.chars().count() > 20_000 {
        let t: String = d.chars().take(20_000).collect();
        return format!("{t}\n… (이하 생략)");
    }
    d
}

/// run 의 시작·끝 스냅샷 트리(`<runs_dir>/<run_id>.log.t0/.t1`). 끝 스냅샷 전(실행 중)이거나 git 저장소가 아니면 None.
pub fn turn_trees(runs_dir: &str, run_id: &str) -> Option<(String, String)> {
    // run_id 는 경로 조각이 된다 — 로그 파일명 문자(숫자·영문·-)만 허용
    if run_id.is_empty() || !run_id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') { return None; }
    let base = format!("{}/{run_id}.log", awb_core::paths::expand_tilde(runs_dir));
    let rd = |ext: &str| std::fs::read_to_string(format!("{base}.{ext}")).ok().map(|s| s.trim().to_string());
    Some((rd("t0")?, rd("t1")?))
}

/// 두 스냅샷 트리 사이의 요약 — 그 턴에 바뀐 것만.
pub fn summarize_turn(workdir: &str, t0: &str, t1: &str) -> DiffSummary {
    let entries: Vec<DiffEntry> = run(&["-C", workdir, "diff", "--name-status", "--no-renames", t0, t1]).lines()
        .filter_map(|l| l.split_once('\t'))
        .map(|(st, path)| DiffEntry { path: path.to_string(), status: st.to_string() })
        .collect();
    let (mut ins, mut del) = (0u32, 0u32);
    for l in run(&["-C", workdir, "diff", "--numstat", t0, t1]).lines() {
        let mut it = l.split_whitespace();
        if let (Some(a), Some(b)) = (it.next(), it.next()) {
            ins += a.parse::<u32>().unwrap_or(0);
            del += b.parse::<u32>().unwrap_or(0);
        }
    }
    DiffSummary { files: entries.len() as u32, insertions: ins, deletions: del, entries }
}

pub fn file_diff_turn(workdir: &str, t0: &str, t1: &str, file: &str) -> String {
    if file.starts_with('/') || file.split('/').any(|c| c == "..") { return String::new(); }
    cap(run(&["-C", workdir, "diff", t0, t1, "--", file]))
}

// ---- 커밋·푸시·PR — 실패하면 git/gh 의 출력을 그대로 돌려준다(폰 토스트에 표시)
use axum::http::StatusCode;
type GitResult = Result<String, (StatusCode, String)>;

fn git(workdir: &str, args: &[&str]) -> GitResult {
    let o = Command::new("git").arg("-C").arg(workdir).args(args)
        .env("GIT_TERMINAL_PROMPT", "0") // 자격 증명 프롬프트로 무기한 멈추지 않게
        .output().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("git 실행 실패: {e}")))?;
    let text = format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr)).trim().to_string();
    if o.status.success() { Ok(text) } else { Err((StatusCode::UNPROCESSABLE_ENTITY, text)) }
}

fn current_branch(workdir: &str) -> Option<String> {
    git(workdir, &["symbolic-ref", "--quiet", "--short", "HEAD"]).ok()
}

/// origin/HEAD 가 가리키는 브랜치, 없으면 main·master 둘 다 기본으로 본다.
pub fn on_default_branch(workdir: &str) -> bool {
    let Some(cur) = current_branch(workdir) else { return false };
    match git(workdir, &["symbolic-ref", "--quiet", "--short", "refs/remotes/origin/HEAD"]) {
        Ok(r) => r.strip_prefix("origin/") == Some(cur.as_str()),
        Err(_) => cur == "main" || cur == "master",
    }
}

pub fn commit(workdir: &str, message: &str, branch: Option<&str>) -> GitResult {
    if message.trim().is_empty() { return Err((StatusCode::BAD_REQUEST, "커밋 메시지가 비었습니다".into())); }
    if awb_core::lock::held(workdir) { return Err((StatusCode::CONFLICT, "턴이 실행 중입니다 — 끝난 뒤 커밋하세요".into())); }
    match branch {
        Some(b) => {
            // --branch 는 '-' 로 시작하는 이름을 거부하고 @{-1} 을 실제 이름으로 펼친다 — 펼쳐지면 입력 그대로가 아니므로 거부
            if git(workdir, &["check-ref-format", "--branch", b]).ok().as_deref() != Some(b) {
                return Err((StatusCode::BAD_REQUEST, format!("브랜치 이름이 올바르지 않습니다: {b}")));
            }
            git(workdir, &["switch", "-c", b])?;
        }
        None if on_default_branch(workdir) => {
            return Err((StatusCode::CONFLICT, "기본 브랜치에는 바로 커밋하지 않습니다 — 새 브랜치 이름을 주세요".into()));
        }
        None => {}
    }
    git(workdir, &["add", "-A"])?;
    git(workdir, &["commit", "-m", message])
}

pub fn push(workdir: &str) -> GitResult {
    if on_default_branch(workdir) { return Err((StatusCode::CONFLICT, "기본 브랜치로는 푸시하지 않습니다".into())); }
    git(workdir, &["push", "-u", "origin", "HEAD"])
}

pub fn pr(workdir: &str, title: &str, body: &str) -> GitResult {
    if title.trim().is_empty() { return Err((StatusCode::BAD_REQUEST, "PR 제목이 비었습니다".into())); }
    let o = Command::new("gh").args(["pr", "create", "--title", title, "--body", body]).current_dir(workdir)
        .env("GH_PROMPT_DISABLED", "1")
        .output().map_err(|e| (StatusCode::UNPROCESSABLE_ENTITY, format!("gh 실행 실패: {e}")))?;
    if o.status.success() { Ok(String::from_utf8_lossy(&o.stdout).trim().to_string()) }
    else { Err((StatusCode::UNPROCESSABLE_ENTITY, String::from_utf8_lossy(&o.stderr).trim().to_string())) }
}

/// 커밋 메시지 초안용 입력 — 커밋은 `add -A` 전체이므로 미추적 파일까지 담긴 스냅샷 트리와 HEAD 를 비교한다. 변경 없으면 None.
pub fn draft_input(workdir: &str) -> Option<String> {
    let tree = awb_core::runlog::snapshot_tree(workdir)?;
    let stat = run(&["-C", workdir, "diff", "--stat", "HEAD", &tree]);
    if stat.trim().is_empty() { return None; }
    let body: String = run(&["-C", workdir, "diff", "HEAD", &tree]).chars().take(4000).collect();
    Some(format!("{stat}\n{body}"))
}

/// haiku 가 프롬프트를 어기고 마크다운을 섞어도 커밋 메시지 칸에는 평문 한 줄만 들어가게 한다.
pub fn clean_draft(out: &str) -> String {
    out.lines()
        .map(|l| l.replace('`', "").trim().trim_matches(|c| matches!(c, '"' | '\'' | '“' | '”' | '‘' | '’')).trim().to_string())
        .find(|l| !l.is_empty())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_draft_strips_markdown_quotes_and_extra_lines() {
        assert_eq!(clean_draft("```\n\"`a.txt` 수정\"\n둘째 줄\n```"), "a.txt 수정");
    }

    #[test]
    fn file_diff_shows_changes_and_blocks_traversal() {
        let wd = std::env::temp_dir().join("awb_filediff_wd"); let _ = std::fs::remove_dir_all(&wd);
        std::fs::create_dir_all(&wd).unwrap();
        let w = wd.to_str().unwrap();
        Command::new("git").args(["-C", w, "init"]).output().unwrap();
        std::fs::write(wd.join("b.txt"), "hello\n").unwrap();
        // 미추적 파일도 /dev/null 대비 diff가 나와야 한다
        let d = file_diff(w, "b.txt");
        assert!(d.contains("+hello"), "{d}");
        // 탈출 경로는 빈 문자열
        assert_eq!(file_diff(w, "../etc/passwd"), "");
        assert_eq!(file_diff(w, "/etc/passwd"), "");
    }
    /// 첫 커밋까지 있는 main 브랜치 저장소
    fn repo(name: &str) -> std::path::PathBuf {
        let wd = std::env::temp_dir().join(name); let _ = std::fs::remove_dir_all(&wd);
        std::fs::create_dir_all(&wd).unwrap();
        let w = wd.to_str().unwrap();
        for a in [&["init", "-b", "main"][..], &["config", "user.email", "t@t"], &["config", "user.name", "t"]] {
            Command::new("git").arg("-C").arg(w).args(a).output().unwrap();
        }
        std::fs::write(wd.join("a.txt"), "base\n").unwrap();
        Command::new("git").args(["-C", w, "add", "-A"]).output().unwrap();
        Command::new("git").args(["-C", w, "commit", "-m", "init"]).output().unwrap();
        wd
    }
    #[test]
    fn turn_diff_shows_only_that_turn() {
        let wd = repo("awb_turn_wd"); let w = wd.to_str().unwrap();
        let runs = std::env::temp_dir().join("awb_turn_runs"); let _ = std::fs::remove_dir_all(&runs);
        std::fs::create_dir_all(&runs).unwrap();
        let log = runs.join("100-1-0.log"); let log = log.to_str().unwrap();
        std::fs::write(log, "").unwrap();
        std::fs::write(wd.join("a.txt"), "base\nbefore turn\n").unwrap(); // 턴 시작 전부터 있던 미커밋 변경
        awb_core::runlog::record_snapshot(w, log, "t0");
        std::fs::write(wd.join("new.txt"), "made in turn\n").unwrap(); // 턴 중 새 미추적 파일
        std::fs::write(format!("{log}.done"), "143").unwrap(); // 취소로 끝나도 끝 스냅샷은 찍혀야 한다
        awb_core::runlog::run_status(log, w);
        let (t0, t1) = turn_trees(runs.to_str().unwrap(), "100-1-0").expect("t0·t1 둘 다 있어야 함");
        let s = summarize_turn(w, &t0, &t1);
        let paths: Vec<_> = s.entries.iter().map(|e| (e.status.as_str(), e.path.as_str())).collect();
        assert_eq!(paths, vec![("A", "new.txt")], "턴 전 변경(a.txt)은 빠져야 함");
        assert!(file_diff_turn(w, &t0, &t1, "new.txt").contains("+made in turn"));
        // 실제 인덱스는 그대로 — new.txt 는 여전히 미추적
        let st = String::from_utf8(Command::new("git").args(["-C", w, "status", "--porcelain"]).output().unwrap().stdout).unwrap();
        assert!(st.contains("?? new.txt"), "{st}");
        assert_eq!(turn_trees(runs.to_str().unwrap(), "../x"), None);
    }
    #[test]
    fn commit_on_default_branch_needs_new_branch() {
        let wd = repo("awb_commit_default_wd"); let w = wd.to_str().unwrap();
        std::fs::write(wd.join("a.txt"), "changed\n").unwrap();
        let r = commit(w, "msg", None);
        assert!(matches!(r, Err((StatusCode::CONFLICT, _))), "{r:?}");
        commit(w, "변경", Some("claude/t")).unwrap();
        assert_eq!(current_branch(w).as_deref(), Some("claude/t"));
        assert_eq!(git(w, &["log", "-1", "--format=%s", "main"]).unwrap(), "init", "main 에는 커밋이 얹히지 않아야 함");
    }
    #[test]
    fn commit_rejects_bad_branch_names() {
        let wd = repo("awb_commit_badname_wd"); let w = wd.to_str().unwrap();
        std::fs::write(wd.join("a.txt"), "changed\n").unwrap();
        for b in ["bad..name", "-f", "@{-1}", "a b"] {
            let r = commit(w, "msg", Some(b));
            assert!(matches!(r, Err((StatusCode::BAD_REQUEST, _))), "{b}: {r:?}");
        }
        assert_eq!(current_branch(w).as_deref(), Some("main"));
    }
    #[test]
    fn counts_changed_files() {
        let wd = std::env::temp_dir().join("awb_diff_wd"); let _ = std::fs::remove_dir_all(&wd);
        std::fs::create_dir_all(&wd).unwrap();
        let w = wd.to_str().unwrap();
        Command::new("git").args(["-C", w, "init"]).output().unwrap();
        std::fs::write(wd.join("a.txt"), "hello\n").unwrap();
        let s = summarize(w);
        assert!(s.files >= 1);                        // 미추적 a.txt 포함
        assert!(s.entries.iter().any(|e| e.path == "a.txt"));
    }
}
