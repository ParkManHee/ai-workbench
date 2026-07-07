# ai-workbench Mobile — Plan 2b: awb-server 실행·스트리밍·푸시

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax.

**Goal:** awb-server 데몬에 claude 실행·실시간 스트리밍·완료 푸시를 추가한다 — `claude --output-format stream-json [--resume]`로 멀티턴 세션을 돌리고, 로그를 JSONL로 append하며, WebSocket으로 토큰을 실시간 전송(오프셋 재접속 이어보기)하고, 완료 시 WS가 없으면 Expo 푸시로 통지한다. 취소·완료판정 포함. Plan 2a의 인증·라우터 위에 얹는다.

**Architecture:** 실행은 **파일 기반 재사용** — awb-core에 `start_stream_run`(기존 `start_run`의 stream-json/resume 변형, 공유 락 그대로) 추가, 래퍼 `scripts/awb-run-stream.sh`가 stream-json을 로그파일에 기록·`.done` 마커. 서버는 로그를 오프셋 tail(awb-core `read_log` 재사용)하며 JSONL 각 줄을 이벤트로 파싱해 WS로 push. 재접속은 `?offset=N`부터 재생(로그가 이벤트 저장소). 완료는 awb-core `run_status`로 판정 + 락 해제. run 레지스트리(run_id→pgid/log/workdir/project)와 세션 스토어(project→session_id)는 서버 상태. Expo 푸시는 완료 워처(run별 tokio task)가 `.done` 감지 후 WS 미전달이면 `curl`로 exp.host에 POST.

**Tech Stack:** Rust, axum 0.8(ws feature 이미 활성), tokio, awb-core(runner/runlog/lock 재사용), serde/serde_json. Expo 푸시는 외부 HTTP를 `curl` 서브프로세스로(코드베이스의 git/tailscale/caffeinate 셸아웃 관행과 일치, 무거운 reqwest 의존 회피). 테스트: 가짜 claude가 stream-json JSONL을 emit하는 스크립트 + `#[tokio::test]`/`oneshot`.

## Global Constraints

- 커밋 트레일러(마지막 줄): `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`
- macOS 우선(유닉스). awb-core 순수성 유지(전송/HTTP 의존 금지 — stream-json 파싱은 순수 로직이라 awb-core 또는 server 어디든 무방하나, WS/HTTP 배선은 server 전용).
- **실행 명령:** `claude -p <prompt> --settings <s> --output-format stream-json [--resume <sid>] [--permission-mode plan]`. claude 경로는 Plan 2a preflight/설정에서 온 값 — 하드코딩 금지.
- **공유 실행 락(runlock) 그대로** — `/chat`은 락 획득(실패 시 409), 완료/취소 시 해제. 앱·폴러·데몬 삼자 이중실행 방지.
- 완료 판정은 exit 단독 금지 — awb-core `run_status`(exit + git 변경파일 수) 재사용.
- 푸시는 **완료 시 WS 미전달일 때만** 발송, run별 `notified` 플래그로 1회. ExpoPushToken은 Bearer 인증된 `/push/register`로만 등록.
- 모든 신규 엔드포인트는 Plan 2a의 `require_token` 뒤(단, WS 인증은 Task 6 참조). `/preflight`(Plan 2a 이월)도 이 플랜에서 추가.

## File Structure (Plan 2b 범위)

```
scripts/awb-run-stream.sh              신규 — stream-json 실행 래퍼(+resume/plan) → 로그 JSONL + .done
crates/awb-core/src/runner.rs          수정 — start_stream_run 추가(락 재사용, stream-json 래퍼 spawn)
crates/awb-server/src/
  sessions.rs                          신규 — 세션 스토어(project→session_id) + stream-json init에서 session_id 파싱
  streamevt.rs                         신규 — JSONL 한 줄 → Event(token/tool_use/done/error) 순수 파서
  runreg.rs                            신규 — run 레지스트리(run_id→RunMeta) + notified 플래그
  routes.rs                            수정 — /chat,/status,/cancel,/preflight,/push/register 핸들러 + AppState 확장
  ws.rs                                신규 — WS /stream/:run_id (offset tail + 이벤트 push + 완료 처리)
  push.rs                              신규 — Expo 푸시 발송(curl) + 완료 워처 spawn
  main.rs                              수정 — mod 등록 + AppState 신규 필드 초기화
```

---

### Task 1: stream-json 실행 래퍼 + awb-core `start_stream_run`

**Files:** Create `scripts/awb-run-stream.sh`; Modify `crates/awb-core/src/runner.rs`(+`start_stream_run`).

**Interfaces:**
- Consumes: `crate::lock::{acquire, LockInfo}`, `crate::paths::expand_tilde`.
- Produces: `awb_core::runner::start_stream_run(claude_bin: &str, workdir: &str, settings: &str, plan: bool, prompt: &str, resume: Option<&str>, runs_dir: &str) -> Result<RunHandle, String>` — 기존 `start_run`과 동일한 락·setsid·meta 로직이되 `awb-run-stream.sh`를 호출하고 `resume`(Some이면 `--resume <sid>`)를 전달. 반환 `RunHandle { log, pgid }`(기존 타입 재사용).

- [ ] **Step 1: 래퍼 스크립트 작성**

Create `scripts/awb-run-stream.sh`:
```sh
#!/bin/sh
# awb-run-stream.sh <claude> <dir> <log> <settings> <plan(0|1)> <resume(''|sid)> <prompt>
CLAUDE="$1"; DIR="$2"; LOG="$3"; SETTINGS="$4"; PLAN="$5"; RESUME="$6"; PROMPT="$7"
cd "$DIR" 2>/dev/null || { echo "127" > "$LOG.done"; exit 127; }
set -- "$CLAUDE" -p "$PROMPT" --settings "$SETTINGS" --output-format stream-json
[ "$PLAN" = "1" ] && set -- "$@" --permission-mode plan
[ -n "$RESUME" ] && set -- "$@" --resume "$RESUME"
"$@" > "$LOG" 2>&1
echo "$?" > "$LOG.done"
```
```bash
chmod +x scripts/awb-run-stream.sh
```

- [ ] **Step 2: 실패 테스트 — 가짜 claude(stream-json 흉내)로 start_stream_run**

Append to `crates/awb-core/src/runner.rs` test module:
```rust
    #[test]
    fn start_stream_run_spawns_with_resume_arg() {
        let dir = std::env::temp_dir().join("awb_stream_proj"); std::fs::create_dir_all(&dir).unwrap();
        let runs = std::env::temp_dir().join("awb_stream_runs"); std::fs::create_dir_all(&runs).unwrap();
        crate::lock::release(dir.to_str().unwrap());
        // 가짜 claude: 인자를 그대로 에코(우리가 --resume/--output-format 전달했는지 확인용) 후 종료
        let fake = std::env::temp_dir().join("fakeclaude_stream");
        std::fs::write(&fake, "#!/bin/sh\nprintf '%s\\n' \"$*\"\n").unwrap();
        #[cfg(unix)]{ use std::os::unix::fs::PermissionsExt; std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap(); }
        let h = start_stream_run(fake.to_str().unwrap(), dir.to_str().unwrap(), "/tmp/ws.json", false, "hi", Some("sess-1"), runs.to_str().unwrap()).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(500));
        let log = std::fs::read_to_string(&h.log).unwrap_or_default();
        assert!(log.contains("--output-format stream-json"));
        assert!(log.contains("--resume sess-1"));
        assert!(std::path::Path::new(&format!("{}.done", h.log)).exists());
        crate::lock::release(dir.to_str().unwrap());
    }
```

- [ ] **Step 3: 실패 확인** — Run: `cargo test -p awb-core start_stream_run_spawns_with_resume_arg` → FAIL.

- [ ] **Step 4: 구현** — Append to `runner.rs`:
```rust
pub fn start_stream_run(claude_bin: &str, workdir: &str, settings: &str, plan: bool, prompt: &str, resume: Option<&str>, runs_dir: &str) -> Result<RunHandle, String> {
    let settings = crate::paths::expand_tilde(settings);
    let runs_dir = crate::paths::expand_tilde(runs_dir);
    let workdir_e = crate::paths::expand_tilde(workdir);
    let placeholder = LockInfo { pid: std::process::id(), pgid: 0, start_ts: now(), source: "daemon".into() };
    if let Err(cur) = acquire(&workdir_e, &placeholder) {
        return Err(format!("이미 실행 중: {} (pid {})", cur.source, cur.pid));
    }
    std::fs::create_dir_all(&runs_dir).map_err(|e| { crate::lock::release(&workdir_e); format!("runs_dir 생성 실패: {e}") })?;
    let log = format!("{}/{}.log", runs_dir, now());
    let wrapper = format!("{}/awb-run-stream.sh", app_dir());
    let plan_flag = if plan { "1" } else { "0" };
    let resume_arg = resume.unwrap_or("");
    let child = unsafe {
        Command::new("sh")
            .args([&wrapper, claude_bin, &workdir_e, &log, &settings, plan_flag, resume_arg, prompt])
            .pre_exec(|| { libc::setsid(); Ok(()) })
            .stdin(std::process::Stdio::null())
            .spawn()
    }.map_err(|e| { crate::lock::release(&workdir_e); format!("spawn 실패: {e}") })?;
    let pgid = child.id() as i32;
    let info = LockInfo { pid: child.id(), pgid, start_ts: now(), source: "daemon".into() };
    let _ = std::fs::write(crate::lock::lock_dir(&workdir_e).join("meta.json"), serde_json::to_string(&info).unwrap());
    Ok(RunHandle { log, pgid })
}
```
`app_dir()`는 기존 함수 재사용(`AWB_SCRIPTS_DIR` 또는 repo scripts/). 테스트에서 `AWB_SCRIPTS_DIR`을 리포 `scripts/`로 지정해야 하면 테스트 셋업에 `std::env::set_var("AWB_SCRIPTS_DIR", <repo>/scripts)` 추가(구현자가 CI 경로에 맞게). 

- [ ] **Step 5: 통과 확인** — Run: `cargo test -p awb-core start_stream_run_spawns_with_resume_arg` → PASS. `cargo test --workspace` → 기존 전부 green.

- [ ] **Step 6: 커밋** — `... -m "feat(core): start_stream_run(stream-json+resume 래퍼, 락 재사용)"` (+트레일러)

---

### Task 2: 세션 스토어 + stream-json init에서 session_id 파싱

**Files:** Create `crates/awb-server/src/sessions.rs`; Modify `main.rs`(`mod sessions;`).

**Interfaces:**
- Produces: `sessions::parse_session_id(jsonl_line: &str) -> Option<String>` — stream-json의 첫 `type":"system"`/init 이벤트에서 `session_id` 추출.
- Produces: `sessions::SessionStore { dir: String }` with `load(dir)`, `get(&self, project) -> Option<String>`, `set(&self, project, sid)`. 저장: `<dir>/<project>.json` = `{"session_id": "..."}`.

- [ ] **Step 1: 실패 테스트 — init 파싱 + 저장/조회**
```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_session_id_from_init() {
        let line = r#"{"type":"system","subtype":"init","session_id":"abc-123","tools":[]}"#;
        assert_eq!(parse_session_id(line), Some("abc-123".to_string()));
        assert_eq!(parse_session_id(r#"{"type":"assistant","message":{}}"#), None);
        assert_eq!(parse_session_id("not json"), None);
    }
    #[test]
    fn session_store_roundtrip() {
        let d = std::env::temp_dir().join("awb_sessions_test"); let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let s = SessionStore::load(d.to_str().unwrap());
        assert_eq!(s.get("proj"), None);
        s.set("proj", "sid-9");
        assert_eq!(s.get("proj"), Some("sid-9".to_string()));
    }
}
```

- [ ] **Step 2: 실패 확인** — `cargo test -p awb-server sessions::` → FAIL.

- [ ] **Step 3: 구현**
```rust
use serde_json::Value;
use std::fs;

pub fn parse_session_id(line: &str) -> Option<String> {
    let v: Value = serde_json::from_str(line).ok()?;
    v.get("session_id")?.as_str().map(|s| s.to_string())
}

#[derive(Clone)]
pub struct SessionStore { pub dir: String }
impl SessionStore {
    pub fn load(dir: &str) -> SessionStore { let _ = fs::create_dir_all(dir); SessionStore { dir: dir.to_string() } }
    fn path(&self, project: &str) -> String {
        let safe: String = project.chars().map(|c| if c.is_alphanumeric() || c=='-' || c=='_' { c } else { '_' }).collect();
        format!("{}/{}.json", self.dir, safe)
    }
    pub fn get(&self, project: &str) -> Option<String> {
        let s = fs::read_to_string(self.path(project)).ok()?;
        let v: Value = serde_json::from_str(&s).ok()?;
        v.get("session_id")?.as_str().map(|x| x.to_string())
    }
    pub fn set(&self, project: &str, sid: &str) {
        let _ = fs::write(self.path(project), format!("{{\"session_id\":{}}}", serde_json::to_string(sid).unwrap()));
    }
}
```

- [ ] **Step 4: 통과 확인** — `cargo test -p awb-server sessions::` → PASS.

- [ ] **Step 5: 커밋** — `... -m "feat(server): 세션 스토어 + stream-json init session_id 파싱"` (+트레일러)

---

### Task 3: stream-json → 이벤트 파서 (`streamevt.rs`)

**Files:** Create `crates/awb-server/src/streamevt.rs`; Modify `main.rs`(`mod streamevt;`).

**Interfaces:**
- Produces: `streamevt::Event`(serde Serialize, `#[serde(tag="kind")]`): `Token{text}`,`ToolUse{name,summary}`,`Done{exit,verdict,changed_files}`,`Error{message}`,`Other`.
- Produces: `streamevt::parse_line(line: &str) -> Option<Event>` — stream-json JSONL 한 줄을 Event로. `type":"assistant"`의 text delta → Token; `tool_use` → ToolUse; 파싱불가/기타 → None(호출자가 skip). (Done은 로그가 아니라 `.done`에서 오므로 여기선 생성 안 함.)

- [ ] **Step 1: 실패 테스트**
```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_assistant_text_to_token() {
        // claude stream-json assistant 이벤트(간략화): content[].text
        let line = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"안녕"}]}}"#;
        match parse_line(line) {
            Some(Event::Token { text }) => assert_eq!(text, "안녕"),
            other => panic!("expected Token, got {other:?}"),
        }
    }
    #[test]
    fn parses_tool_use() {
        let line = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Bash","input":{"command":"ls"}}]}}"#;
        match parse_line(line) {
            Some(Event::ToolUse { name, .. }) => assert_eq!(name, "Bash"),
            other => panic!("expected ToolUse, got {other:?}"),
        }
    }
    #[test]
    fn ignores_unparseable() { assert!(parse_line("garbage").is_none()); }
}
```

- [ ] **Step 2: 실패 확인** — `cargo test -p awb-server streamevt::` → FAIL.

- [ ] **Step 3: 구현**
```rust
use serde::Serialize;
use serde_json::Value;

#[derive(Serialize, Debug, Clone)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Event {
    Token { text: String },
    ToolUse { name: String, summary: String },
    Done { exit: Option<i32>, verdict: String, changed_files: u32 },
    Error { message: String },
}

pub fn parse_line(line: &str) -> Option<Event> {
    let v: Value = serde_json::from_str(line).ok()?;
    match v.get("type").and_then(|t| t.as_str()) {
        Some("assistant") => {
            let content = v.get("message")?.get("content")?.as_array()?;
            for item in content {
                match item.get("type").and_then(|t| t.as_str()) {
                    Some("text") => {
                        if let Some(t) = item.get("text").and_then(|x| x.as_str()) {
                            return Some(Event::Token { text: t.to_string() });
                        }
                    }
                    Some("tool_use") => {
                        let name = item.get("name").and_then(|x| x.as_str()).unwrap_or("tool").to_string();
                        let summary = item.get("input").map(|i| i.to_string()).unwrap_or_default();
                        let summary = if summary.len() > 200 { format!("{}…", &summary[..200]) } else { summary };
                        return Some(Event::ToolUse { name, summary });
                    }
                    _ => {}
                }
            }
            None
        }
        Some("result") => v.get("subtype").and_then(|s| s.as_str())
            .filter(|s| s.contains("error"))
            .map(|_| Event::Error { message: v.get("error").map(|e| e.to_string()).unwrap_or_else(|| "error".into()) }),
        _ => None,
    }
}
```
(주: `summary[..200]`은 UTF-8 경계에서 패닉 가능 — 구현자는 `char_indices`로 안전 절단할 것. Done/verdict는 WS 핸들러가 `.done`+run_status로 생성.)

- [ ] **Step 4: 통과 확인** — `cargo test -p awb-server streamevt::` → PASS.

- [ ] **Step 5: 커밋** — `... -m "feat(server): stream-json JSONL → 이벤트 파서(token/tool_use/error)"` (+트레일러)

---

### Task 4: run 레지스트리 + /chat + /status + /cancel + /preflight

**Files:** Create `crates/awb-server/src/runreg.rs`; Modify `routes.rs`(핸들러·AppState), `main.rs`.

**Interfaces:**
- Produces: `runreg::RunMeta { log: String, pgid: i32, workdir: String, project: String, notified: bool }`, `runreg::RunRegistry`(Arc<Mutex<HashMap<String, RunMeta>>> 래퍼) with `insert(run_id, meta)`, `get(run_id) -> Option<RunMeta>`, `mark_notified(run_id) -> bool`(false→true 전이 시 true 반환; 1회 보장), `remove(run_id)`.
- Produces: `routes::chat_handler`(`POST /chat/:project` body `{prompt, plan?}` → `{run_id, log}`; awb-core `start_stream_run`으로 락 획득·실행, 세션 스토어의 resume sid 사용, run 등록), `status_handler`(`GET /status/:run_id` → awb-core `run_status`), `cancel_handler`(`POST /cancel/:run_id` → awb-core `cancel_run`), `preflight_handler`(`GET /preflight` → awb-core `run_preflight`).
- Produces: `AppState` 확장 — `sessions: SessionStore`, `runs: RunRegistry`, `claude_bin: String`, `settings_path: String`, `runs_dir: String`.

- [ ] **Step 1: 실패 테스트 — 레지스트리 notified 1회 전이**
```rust
// runreg.rs 테스트
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mark_notified_transitions_once() {
        let r = RunRegistry::new();
        r.insert("run1", RunMeta { log: "l".into(), pgid: 10, workdir: "w".into(), project: "p".into(), notified: false });
        assert!(r.mark_notified("run1"));   // 첫 호출 true
        assert!(!r.mark_notified("run1"));   // 두번째 false(중복 방지)
        assert!(!r.mark_notified("absent")); // 없는 run false
    }
}
```

- [ ] **Step 2: 실패 확인** — `cargo test -p awb-server runreg::` → FAIL.

- [ ] **Step 3: 구현 (runreg.rs)**
```rust
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct RunMeta { pub log: String, pub pgid: i32, pub workdir: String, pub project: String, pub notified: bool }

#[derive(Clone)]
pub struct RunRegistry { inner: Arc<Mutex<HashMap<String, RunMeta>>> }
impl RunRegistry {
    pub fn new() -> RunRegistry { RunRegistry { inner: Arc::new(Mutex::new(HashMap::new())) } }
    pub fn insert(&self, run_id: &str, meta: RunMeta) { self.inner.lock().unwrap().insert(run_id.to_string(), meta); }
    pub fn get(&self, run_id: &str) -> Option<RunMeta> { self.inner.lock().unwrap().get(run_id).cloned() }
    pub fn mark_notified(&self, run_id: &str) -> bool {
        let mut g = self.inner.lock().unwrap();
        match g.get_mut(run_id) { Some(m) if !m.notified => { m.notified = true; true }, _ => false }
    }
    pub fn remove(&self, run_id: &str) { self.inner.lock().unwrap().remove(run_id); }
}
```

- [ ] **Step 4: /chat·/status·/cancel·/preflight 핸들러 (routes.rs) + AppState 확장**
```rust
// AppState 최종 확장(Plan 2a 필드 + 아래 추가)
//   sessions: SessionStore, runs: RunRegistry, claude_bin: String, settings_path: String, runs_dir: String
use axum::extract::{Path, State};
use axum::{Json, http::StatusCode};

#[derive(serde::Deserialize)]
pub struct ChatBody { pub prompt: String, #[serde(default)] pub plan: bool }
#[derive(serde::Serialize)]
pub struct ChatResult { pub run_id: String, pub log: String }

pub async fn chat_handler(State(st): State<AppState>, Path(project): Path<String>, Json(b): Json<ChatBody>) -> Result<Json<ChatResult>, (StatusCode, String)> {
    // 프로젝트 경로 확인
    let proj = awb_core::scan::scan_roots(&st.roots).into_iter().find(|p| p.name == project)
        .ok_or((StatusCode::NOT_FOUND, "unknown project".into()))?;
    let resume = st.sessions.get(&project);
    let h = awb_core::runner::start_stream_run(&st.claude_bin, &proj.path, &st.settings_path, b.plan, &b.prompt, resume.as_deref(), &st.runs_dir)
        .map_err(|e| (StatusCode::CONFLICT, e))?;
    let run_id = h.log.rsplit('/').next().unwrap_or(&h.log).trim_end_matches(".log").to_string();
    st.runs.insert(&run_id, crate::runreg::RunMeta { log: h.log.clone(), pgid: h.pgid, workdir: proj.path.clone(), project: project.clone(), notified: false });
    // 완료 워처(푸시) spawn — Task 7에서 push::spawn_watch 로 연결. 이 태스크에선 등록만.
    Ok(Json(ChatResult { run_id, log: h.log }))
}

pub async fn status_handler(State(st): State<AppState>, Path(run_id): Path<String>) -> Result<Json<awb_core::runlog::RunStatus>, StatusCode> {
    let meta = st.runs.get(&run_id).ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(awb_core::runlog::run_status(&meta.log, &meta.workdir)))
}

pub async fn cancel_handler(State(st): State<AppState>, Path(run_id): Path<String>) -> Result<StatusCode, StatusCode> {
    let meta = st.runs.get(&run_id).ok_or(StatusCode::NOT_FOUND)?;
    let dead = awb_core::runner::cancel_run(meta.pgid, &meta.workdir);
    Ok(if dead { StatusCode::OK } else { StatusCode::ACCEPTED })
}

pub async fn preflight_handler(State(st): State<AppState>) -> Json<awb_core::preflight::Preflight> {
    Json(awb_core::preflight::run_preflight(&st.roots, Some(st.claude_bin.clone())))
}
```
라우터에 인증 뒤로 추가: `.route("/chat/{project}", post(chat_handler)).route("/status/{run_id}", get(status_handler)).route("/cancel/{run_id}", post(cancel_handler)).route("/preflight", get(preflight_handler))` (axum 0.8 path 파라미터 문법 `{name}`).

- [ ] **Step 5: 통과 확인** — `cargo test -p awb-server` → PASS(runreg 포함). `cargo build -p awb-server` → 성공.

- [ ] **Step 6: 커밋** — `... -m "feat(server): run 레지스트리 + /chat·/status·/cancel·/preflight"` (+트레일러)

---

### Task 5: WS 스트리밍 (`ws.rs`) — /stream/:run_id

**Files:** Create `crates/awb-server/src/ws.rs`; Modify `routes.rs`(라우트), `main.rs`(`mod ws;`).

**Interfaces:**
- Consumes: `awb_core::runlog::{read_log, run_status}`, `streamevt::parse_line`, `runreg`, 세션 스토어(완료 시 session_id 갱신).
- Produces: `ws::stream_handler` — `GET /stream/:run_id?offset=N` WS 업그레이드. 루프: `read_log(log, offset)`로 신규 바이트 → 줄 단위 분해 → 각 줄 `parse_line` → Some이면 WS 텍스트로 send(JSON), init 줄이면 `sessions.set`. `chunk.done`이면 `run_status`로 verdict 계산해 `Event::Done` send → 락은 run_status가 해제 → `runs.mark_notified(run_id)`(WS가 전달했음 표시) → WS 종료. 미완료면 ~1s sleep 후 반복.
- **WS 인증:** 브라우저 WS는 헤더 제약이 있으나 RN 클라이언트는 헤더 전송 가능 → `Authorization: Bearer` 또는 `?token=` 쿼리 허용. 핸들러 진입 시 토큰 검증(미들웨어가 WS 업그레이드에 안 걸리면 핸들러 내 수동 verify), 실패 시 close.

- [ ] **Step 1: 실패 테스트 — 줄 분해 누적 로직(순수 부분 추출)**
WS 자체는 통합테스트가 무거우므로 순수 로직을 분리해 테스트:
```rust
// ws.rs: pub fn split_new_lines(buf: &str) -> (Vec<String>, usize)  — 완결된 줄들 + 소비한 바이트수(마지막 미완결 줄 남김)
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn splits_only_complete_lines() {
        let (lines, consumed) = split_new_lines("a\nb\npartial");
        assert_eq!(lines, vec!["a".to_string(), "b".to_string()]);
        assert_eq!(consumed, 4); // "a\nb\n"
    }
}
```

- [ ] **Step 2: 실패 확인** — `cargo test -p awb-server ws::` → FAIL.

- [ ] **Step 3: 구현** — `split_new_lines` + WS 핸들러:
```rust
use axum::extract::ws::{WebSocket, WebSocketUpgrade, Message};
use axum::extract::{Path, Query, State};
use axum::response::Response;
use crate::routes::AppState;

pub fn split_new_lines(buf: &str) -> (Vec<String>, usize) {
    let mut lines = Vec::new(); let mut consumed = 0;
    for line in buf.split_inclusive('\n') {
        if line.ends_with('\n') { lines.push(line.trim_end_matches('\n').to_string()); consumed += line.len(); }
    }
    (lines, consumed)
}

#[derive(serde::Deserialize)]
pub struct StreamQuery { #[serde(default)] pub offset: u64, pub token: Option<String> }

pub async fn stream_handler(
    ws: WebSocketUpgrade,
    State(st): State<AppState>,
    Path(run_id): Path<String>,
    Query(q): Query<StreamQuery>,
) -> Response {
    // WS 토큰 검증(쿼리 token) — 미들웨어가 업그레이드에 안 걸리므로 수동
    if !q.token.map(|t| st.devices.verify(&t)).unwrap_or(false) {
        return axum::http::StatusCode::UNAUTHORIZED.into_response();
    }
    let meta = st.runs.get(&run_id);
    ws.on_upgrade(move |socket| stream_loop(socket, st, run_id, q.offset, meta))
}

async fn stream_loop(mut socket: WebSocket, st: AppState, run_id: String, mut offset: u64, meta: Option<crate::runreg::RunMeta>) {
    let meta = match meta { Some(m) => m, None => { let _ = socket.send(Message::Text("{\"kind\":\"error\",\"message\":\"unknown run\"}".into())).await; return; } };
    let mut pending = String::new();
    loop {
        let chunk = awb_core::runlog::read_log(&meta.log, offset);
        offset = chunk.offset;
        if !chunk.text.is_empty() {
            pending.push_str(&chunk.text);
            let (lines, consumed) = split_new_lines(&pending);
            pending = pending[consumed..].to_string();
            for line in lines {
                if let Some(sid) = crate::sessions::parse_session_id(&line) { st.sessions.set(&meta.project, &sid); }
                if let Some(ev) = crate::streamevt::parse_line(&line) {
                    let _ = socket.send(Message::Text(serde_json::to_string(&ev).unwrap().into())).await;
                }
            }
        }
        if chunk.done {
            let status = awb_core::runlog::run_status(&meta.log, &meta.workdir); // 락 해제 포함
            let done = crate::streamevt::Event::Done { exit: status.exit_code, verdict: status.verdict.clone(), changed_files: status.changed_files };
            let _ = socket.send(Message::Text(serde_json::to_string(&done).unwrap().into())).await;
            st.runs.mark_notified(&run_id); // WS로 전달됨 → 푸시 스킵
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
    }
}
```
라우트 추가(인증 미들웨어 밖 — 자체 토큰검증): 완성 라우터에서 `/pair`와 같은 무미들웨어 그룹에 `.route("/stream/{run_id}", get(crate::ws::stream_handler))` 추가. `use axum::response::IntoResponse;` 필요.

- [ ] **Step 4: 통과 확인** — `cargo test -p awb-server ws::split_new_lines` → PASS. `cargo build -p awb-server` → 성공.

- [ ] **Step 5: 스모크(수동)** — 가짜 stream-json을 쓰는 run으로 `websocat "ws://127.0.0.1:8787/stream/<id>?token=<t>"` 접속해 이벤트 수신 확인(구현자 환경 가능 시). 불가하면 build+단위테스트로 대체 명시.

- [ ] **Step 6: 커밋** — `... -m "feat(server): WS /stream 실시간 이벤트(offset 재접속 이어보기)"` (+트레일러)

---

### Task 6: Expo 푸시 + /push/register + 완료 워처

**Files:** Create `crates/awb-server/src/push.rs`; Modify `routes.rs`(/push/register), `main.rs`, `chat_handler`(워처 spawn).

**Interfaces:**
- Produces: `push::PushStore { path: String }`(`~/.claude/.awb-push-tokens.json`, ExpoPushToken 목록) with `add(token)`, `list()`.
- Produces: `push::send(tokens: &[String], title: &str, body: &str)` — `curl -s -X POST https://exp.host/--/api/v2/push/send -H 'Content-Type: application/json' -d '[{to,title,body}...]'`(서브프로세스). 실패는 로그만.
- Produces: `push::spawn_watch(st, run_id)` — tokio task: `.done` 뜰 때까지 폴링(read_log done) → `run_status` verdict → `runs.mark_notified(run_id)`가 true면(=아직 WS가 안 가져감) `push::send`로 발송. false면(WS가 이미 전달) 스킵.
- Produces: `routes::push_register_handler`(`POST /push/register` body `{token}` → PushStore.add).
- `chat_handler`가 run 등록 직후 `push::spawn_watch(st.clone(), run_id.clone())` 호출.

- [ ] **Step 1: 실패 테스트 — 발송 결정 로직(mark_notified 게이트)**
```rust
// push.rs: pub fn should_push(reg: &RunRegistry, run_id: &str) -> bool { reg.mark_notified(run_id) }
#[cfg(test)]
mod tests {
    use super::*;
    use crate::runreg::{RunRegistry, RunMeta};
    #[test]
    fn pushes_only_if_ws_did_not_consume() {
        let r = RunRegistry::new();
        r.insert("x", RunMeta { log:"l".into(), pgid:1, workdir:"w".into(), project:"p".into(), notified:false });
        // WS가 먼저 전달한 경우
        assert!(r.mark_notified("x"));       // WS가 소비
        assert!(!should_push(&r, "x"));      // 워처는 스킵
        // WS가 없던 경우
        r.insert("y", RunMeta { log:"l".into(), pgid:1, workdir:"w".into(), project:"p".into(), notified:false });
        assert!(should_push(&r, "y"));       // 워처가 발송
    }
}
```

- [ ] **Step 2: 실패 확인** — `cargo test -p awb-server push::` → FAIL.

- [ ] **Step 3: 구현 (push.rs)** — `PushStore`(auth.rs의 파일 패턴 참고), `should_push`, `send`(curl), `spawn_watch`:
```rust
pub fn should_push(reg: &crate::runreg::RunRegistry, run_id: &str) -> bool { reg.mark_notified(run_id) }

pub fn send(tokens: &[String], title: &str, body: &str) {
    if tokens.is_empty() { return; }
    let msgs: Vec<_> = tokens.iter().map(|t| serde_json::json!({"to": t, "title": title, "body": body})).collect();
    let payload = serde_json::to_string(&msgs).unwrap_or_default();
    let _ = std::process::Command::new("curl")
        .args(["-s","-X","POST","https://exp.host/--/api/v2/push/send","-H","Content-Type: application/json","-d",&payload])
        .output();
}

pub fn spawn_watch(st: crate::routes::AppState, run_id: String) {
    tokio::spawn(async move {
        let meta = match st.runs.get(&run_id) { Some(m) => m, None => return };
        loop {
            let chunk = awb_core::runlog::read_log(&meta.log, 0);
            if chunk.done {
                let status = awb_core::runlog::run_status(&meta.log, &meta.workdir);
                if should_push(&st.runs, &run_id) {
                    let tokens = st.push.list();
                    let title = format!("{} {}", if status.verdict.starts_with("success") {"✅"} else {"❌"}, meta.project);
                    push_body_and_send(&tokens, &title, &status.verdict);
                }
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
        }
    });
}
fn push_body_and_send(tokens: &[String], title: &str, verdict: &str) { send(tokens, title, verdict); }
```
`PushStore`는 `add`/`list`(중복 토큰 무시). `routes::push_register_handler`는 인증 뒤 라우트. `AppState`에 `push: PushStore` 추가. `chat_handler` 끝에 `crate::push::spawn_watch(st.clone(), run_id.clone());`.

- [ ] **Step 4: 통과 확인** — `cargo test -p awb-server push::` → PASS. `cargo test --workspace` → 전부 green. `cargo build -p awb-server` → 성공.

- [ ] **Step 5: 커밋** — `... -m "feat(server): Expo 푸시(완료 워처+notified 게이트) + /push/register"` (+트레일러)

---

### Task 7: main 배선 통합 + 전체 라우터 확정 + 워크스페이스 스모크

**Files:** Modify `crates/awb-server/src/main.rs`(AppState 전체 필드 초기화), `routes.rs`(최종 라우터에 신규 라우트 통합).

**Interfaces:** 최종 `router(state)` — 무인증군: `/pair`,`/stream/{run_id}`(자체 토큰검증). 인증군: `/health`,`/projects`,`/diff`,`/awake`,`/chat/{project}`,`/status/{run_id}`,`/cancel/{run_id}`,`/preflight`,`/push/register`. `serve()`에서 `AppState`를 모든 신규 필드(sessions/runs/push/claude_bin/settings_path/runs_dir)로 초기화.

- [ ] **Step 1: claude_bin 결정** — `serve()`에서 claude 경로 resolve: awb-core preflight의 결과(`run_preflight(&roots, None).claude_path`) 사용, 없으면 `AWB_CLAUDE_BIN` env, 최종 폴백 `"claude"`. settings_path=`~/.claude/worker-settings.json`, runs_dir=cfg.runs_dir.

- [ ] **Step 2: AppState 초기화 + 라우터 확정** — 모든 필드 채워 `routes::router(state)` 호출. 신규 라우트 등록 확인.

- [ ] **Step 3: 통과 확인** — `cargo test --workspace` → 전부 green(awb-core + awb-server 전체). `cargo build --workspace` → 성공. `cargo clippy -p awb-server` → 신규 경고 0 목표(불가피한 건 보고).

- [ ] **Step 4: 스모크(수동)** — `cargo run -p awb-server -- serve`: 페어링 QR + 서빙. 가짜 claude로 `/chat` → run_id → `/stream?token=` 이벤트 → 완료 배지. (헤드리스면 build+workspace 테스트로 대체 명시, 실 claude/기기 스모크는 사용자 최종 검증으로 이월.)

- [ ] **Step 5: 커밋** — `... -m "feat(server): 실행/스트림/푸시 배선 통합 + 전체 라우터 확정(Plan 2b)"` (+트레일러)

---

## Plan 2b Self-Review

- **Spec coverage:** stream-json 러너=Task1; 세션(`--resume`)=Task2/4; JSONL→이벤트=Task3; `/chat`·락 획득=Task4; `/status`·`/cancel`=Task4; WS `/stream`·offset 재접속=Task5; Expo 푸시(WS 미전달 시·notified 1회)·`/push/register`=Task6; `/preflight`(Plan 2a 이월)=Task4; 배선=Task7. runlock 재사용=Task1/4/5. (v2 이월: iOS/APNs·WoL·턴 큐잉·히스토리 영구저장·diff 뷰어 — 범위 밖.)
- **Placeholder scan:** 각 스텝 실제 코드·명령·기대. TODO: WS 인증(쿼리 token)·UTF-8 안전절단은 본문에 명시. Done 이벤트는 로그 아닌 `.done`+run_status에서 생성함을 Task3/5에 명시.
- **Type consistency:** `RunHandle`(log/pgid) Task1↔4; `RunMeta`(log/pgid/workdir/project/notified) Task4↔5↔6; `Event`(token/tool_use/done/error) Task3↔5; `RunRegistry.mark_notified` 1회전이 계약 Task4↔6(should_push가 이를 게이트); `run_status`(exit/verdict/changed_files) awb-core↔Task4/5. AppState 최종 필드(Plan2a: devices/pairing/roots/power + 2b: sessions/runs/push/claude_bin/settings_path/runs_dir) Task4/6/7 일관.
- **위험:** (1) claude stream-json 실제 스키마와 파서 필드 차이 — 가짜 스트림으로 단위테스트하되 실제 스키마는 사용자 스모크에서 검증(구현자는 claude 실제 출력 샘플로 파서 조정 가능). (2) WS 인증을 쿼리 token으로 — tailnet+토큰 이중방어 유지(로그에 token 남기지 않도록 주의). (3) 완료 워처와 WS의 경쟁 → `mark_notified` 원자적 1회 전이로 중복 푸시 차단. (4) awb-core에 daemon source 추가되지만 여전히 순수(HTTP 없음) 유지.

## 다음 플랜
- **Plan 3 (mobile RN+Expo 앱):** 페어링(QR 스캔)·프로젝트목록·채팅(멀티턴)·WS 스트리밍 수신·plan토글·취소·완료배지·git요약·Expo 푸시 등록. Plan 2a/2b API 소비. ⚠️ 실기 구동·EAS APK·FCM 크리덴셜은 사용자 기기·계정 필요.
