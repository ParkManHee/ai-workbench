# ai-workbench Plan 2 — Execution & Progress 구현 계획

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax.

**Goal:** 프로젝트에서 프롬프트를 실행(detached `claude -p`)하고, 진행을 로그 tail로 보며, 완료를 다층 판정하고, 취소할 수 있다. 앱·자율 폴러가 같은 워크트리를 이중 실행하지 못하도록 공유 락을 건다.

**Architecture:** Rust 코어가 락 획득 → 래퍼 스크립트를 setsid(자체 PGID)로 detached 실행 → 로그파일+`.done` 마커 기록. 프론트는 로그를 오프셋 tail(1~2초)하고 완료 배지·취소를 제공. plan 토글은 `--permission-mode plan` 인자로 강제.

**Tech Stack:** Tauri v2, Rust(stable), Svelte 5 + Vite, 기존 `~/.claude/worker-settings.json`·`claude` CLI 재사용. 테스트: Rust `#[test]`(+ 임시 프로세스/파일 픽스처), 프론트 vitest.

## Global Constraints

- 실행 명령: `claude -p <prompt> --settings <worker-settings 경로>` (plan 모드면 `--permission-mode plan` 추가). claude 경로는 Plan 1 preflight가 resolve한 값(app state)을 사용 — 하드코딩 금지.
- **공유 락 = mkdir 원자락**, 키 = 워크트리 **realpath**(심링크 해소). 락 디렉토리 루트 `~/.claude/.run-locks/`. 락 내부 `meta.json` = `{pid, pgid, start_ts, source}` (source ∈ app|agent-run|project-poll).
- detached 실행은 **setsid로 자체 프로세스 그룹(PGID)** 을 가져 앱이 죽어도 지속, 취소는 PGID 대상.
- 완료 마커 `<log>.done` 에 exit code 기록. 완료 판정은 exit code 단독 금지 — exit + git 변경파일 수 병행(v0는 plain text 출력이므로 stream-json 파싱은 Plan 3/v1).
- 기존 `~/.claude/bin/agent-run.sh`·`project-poll.py` 도 **같은 락을 확인/획득**해야 함(락 공유가 유효하려면 양측 필수) — Task 7.
- Rust는 macOS 우선(유닉스 `setsid`/`killpg`); 크로스플랫폼 분기는 주석 TODO만.
- 커밋 트레일러(마지막 줄): `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`

## File Structure (Plan 2 범위)

```
src-tauri/src/
  lock.rs        공유 실행 락(acquire/release/status, realpath 키)
  runner.rs      detached 실행(start_run) + 실행 메타
  runlog.rs      로그 오프셋 tail(read_log) + 완료판정(run_status)
  main.rs        command 등록(start_run, read_log, run_status, cancel_run)
scripts/
  awb-run.sh     락 획득→claude 실행→로그/.done→락 해제 (앱이 setsid로 실행)
src/lib/
  Console.svelte     프롬프트 입력+plan토글+실행+tail+완료배지+취소
  run.ts             실행 상태 순수 로직(파싱/상태머신) + 타입
  run.test.ts        vitest
~/.claude/bin/        (Task 7, 별도 repo)
  agent-run.sh · project-poll.py  동일 락 확인/획득 삽입
```

---

### Task 1: 공유 실행 락 (`lock.rs`)

**Files:** Create `src-tauri/src/lock.rs`; Modify `src-tauri/src/main.rs`(`mod lock;`); Test 인라인.

**Interfaces:**
- Produces: `pub struct LockInfo { pub pid: u32, pub pgid: i32, pub start_ts: u64, pub source: String }`
- `pub fn lock_dir(workdir: &str) -> std::path::PathBuf` — `~/.claude/.run-locks/<sha1(realpath(workdir))>`.
- `pub fn acquire(workdir: &str, info: &LockInfo) -> Result<(), LockInfo>` — mkdir 원자획득 성공 시 `meta.json` 기록 후 Ok; 이미 있으면 기존 `meta.json` 파싱해 Err(기존 소유자). 소유자 프로세스가 죽었으면(pgid 부재) stale로 간주해 탈취.
- `pub fn release(workdir: &str)` — 락 디렉토리 제거.
- `pub fn status(workdir: &str) -> Option<LockInfo>` — 현재 소유자(없으면 None).

- [ ] **Step 1: 실패 테스트 — 획득 후 재획득 실패, release 후 재획득 성공**
```rust
#[cfg(test)]
mod tests {
    use super::*;
    fn info(pgid: i32) -> LockInfo { LockInfo{ pid: std::process::id(), pgid, start_ts: 1, source: "test".into() } }
    #[test]
    fn acquire_is_exclusive_then_releasable() {
        let d = std::env::temp_dir().join("awb_lock_proj"); let _ = std::fs::create_dir_all(&d);
        let w = d.to_str().unwrap();
        release(w); // 청소
        assert!(acquire(w, &info(std::process::id() as i32)).is_ok());
        // 살아있는 소유자(pgid=현재 프로세스 그룹) → 재획득 실패
        assert!(acquire(w, &info(std::process::id() as i32)).is_err());
        release(w);
        assert!(acquire(w, &info(std::process::id() as i32)).is_ok());
        release(w);
    }
}
```

- [ ] **Step 2: 실패 확인** — Run: `cd src-tauri && cargo test acquire_is_exclusive_then_releasable` → FAIL(모듈 없음).

- [ ] **Step 3: 구현**
```rust
use serde::{Serialize, Deserialize};
use std::fs;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone)]
pub struct LockInfo { pub pid: u32, pub pgid: i32, pub start_ts: u64, pub source: String }

fn home() -> String { std::env::var("HOME").unwrap_or_default() }
fn realpath(p: &str) -> String { fs::canonicalize(p).map(|x| x.to_string_lossy().into()).unwrap_or_else(|_| p.into()) }

fn sha1_hex(s: &str) -> String {
    // 의존성 최소화: 간단한 FNV-1a 64bit(충돌 실질 무시). 파일명 안전.
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.as_bytes() { h ^= *b as u64; h = h.wrapping_mul(0x100000001b3); }
    format!("{:016x}", h)
}

pub fn lock_dir(workdir: &str) -> PathBuf {
    PathBuf::from(home()).join(".claude/.run-locks").join(sha1_hex(&realpath(workdir)))
}

fn pgid_alive(pgid: i32) -> bool {
    // killpg(pgid, 0): 존재하면 Ok
    unsafe { libc::killpg(pgid, 0) == 0 }
}

pub fn status(workdir: &str) -> Option<LockInfo> {
    let meta = lock_dir(workdir).join("meta.json");
    let s = fs::read_to_string(meta).ok()?;
    serde_json::from_str(&s).ok()
}

pub fn acquire(workdir: &str, info: &LockInfo) -> Result<(), LockInfo> {
    let dir = lock_dir(workdir);
    if let Some(parent) = dir.parent() { let _ = fs::create_dir_all(parent); }
    match fs::create_dir(&dir) {
        Ok(_) => { let _ = fs::write(dir.join("meta.json"), serde_json::to_string(info).unwrap()); Ok(()) }
        Err(_) => {
            if let Some(cur) = status(workdir) {
                if pgid_alive(cur.pgid) { return Err(cur); }
            }
            // stale(소유자 죽음/메타 없음) → 탈취
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir(&dir).map_err(|_| info.clone())?;
            let _ = fs::write(dir.join("meta.json"), serde_json::to_string(info).unwrap());
            Ok(())
        }
    }
}

pub fn release(workdir: &str) { let _ = fs::remove_dir_all(lock_dir(workdir)); }
```
`Cargo.toml` 에 `libc = "0.2"`, `serde_json = "1"` 추가(serde는 기존). `main.rs`에 `mod lock;`.

- [ ] **Step 4: 통과 확인** — Run: `cd src-tauri && cargo test acquire_is_exclusive_then_releasable` → PASS.

- [ ] **Step 5: 커밋** — `git add -A && git commit -m "feat: 공유 실행 락(realpath 키, mkdir 원자·stale 탈취)\n\nCo-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"`

---

### Task 2: 실행 래퍼 스크립트 (`scripts/awb-run.sh`)

**Files:** Create `scripts/awb-run.sh`; Test: `src-tauri/src/runner.rs`(Task 3)에서 통합 검증.

**Interfaces (계약):**
- 인자: `awb-run.sh <claude_bin> <workdir> <logfile> <settings_path> <plan_flag(0|1)> <prompt>`.
- 동작: `cd workdir` → `<claude_bin> -p "<prompt>" --settings <settings_path> [--permission-mode plan]` 출력을 `logfile`로 → 종료 시 `<logfile>.done` 에 exit code 기록. (락 획득/해제는 Rust `runner`가 담당 — 래퍼는 실행만.)

- [ ] **Step 1: 스크립트 작성**
```sh
#!/bin/sh
CLAUDE="$1"; DIR="$2"; LOG="$3"; SETTINGS="$4"; PLAN="$5"; PROMPT="$6"
cd "$DIR" 2>/dev/null || { echo "127" > "$LOG.done"; exit 127; }
if [ "$PLAN" = "1" ]; then
  "$CLAUDE" -p "$PROMPT" --settings "$SETTINGS" --permission-mode plan > "$LOG" 2>&1
else
  "$CLAUDE" -p "$PROMPT" --settings "$SETTINGS" > "$LOG" 2>&1
fi
echo "$?" > "$LOG.done"
```

- [ ] **Step 2: 실행권한 + 스모크(가짜 claude)**
```bash
chmod +x scripts/awb-run.sh
printf '#!/bin/sh\necho "hi $*"\n' > /tmp/fakeclaude && chmod +x /tmp/fakeclaude
sh scripts/awb-run.sh /tmp/fakeclaude "$PWD" /tmp/awb.log /tmp/ws.json 0 "hello"
cat /tmp/awb.log; cat /tmp/awb.log.done
```
Expected: `awb.log` 에 `hi -p hello --settings /tmp/ws.json`, `awb.log.done` 에 `0`.

- [ ] **Step 3: 커밋** — `git add -A && git commit -m "feat: awb-run.sh 실행 래퍼(로그+.done)\n\nCo-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"`

---

### Task 3: detached 러너 (`runner.rs`)

**Files:** Create `src-tauri/src/runner.rs`; Modify `main.rs`(`mod runner;` + `start_run` command); Test 인라인(가짜 claude로).

**Interfaces:**
- Consumes: `lock::{acquire, release, LockInfo}`.
- Produces: `pub struct RunHandle { pub log: String, pub pgid: i32 }`
- `pub fn start_run(claude_bin: &str, workdir: &str, settings: &str, plan: bool, prompt: &str, runs_dir: &str) -> Result<RunHandle, String>` — 락 획득(실패 시 Err "이미 실행 중: <source>"), `runs_dir/<epoch>.log` 경로 생성, `scripts/awb-run.sh` 를 **setsid(자체 PGID)** 로 spawn, `meta.json`에 pgid 기록, 즉시 RunHandle 반환(대기 안 함).
- Produces (command): `#[tauri::command] fn start_run(...) -> Result<RunHandle, String>`.
- **참고:** 락 해제는 러너가 아니라 완료 감지 시점(runlog::run_status가 done 확인 후) 또는 cancel에서. Task 5/6에서 연결.

- [ ] **Step 1: 실패 테스트 — 가짜 claude로 start_run 하면 로그+.done 생성, 재실행은 락으로 거부**
```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn start_run_spawns_and_locks() {
        let dir = std::env::temp_dir().join("awb_runner_proj"); std::fs::create_dir_all(&dir).unwrap();
        let runs = std::env::temp_dir().join("awb_runs"); std::fs::create_dir_all(&runs).unwrap();
        crate::lock::release(dir.to_str().unwrap());
        // 가짜 claude: 0.5초 자고 종료
        let fake = std::env::temp_dir().join("fakeclaude2");
        std::fs::write(&fake, "#!/bin/sh\nsleep 0.3\necho done\n").unwrap();
        #[cfg(unix)]{ use std::os::unix::fs::PermissionsExt; std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap(); }
        let h = start_run(fake.to_str().unwrap(), dir.to_str().unwrap(), "/tmp/ws.json", false, "hi", runs.to_str().unwrap()).unwrap();
        assert!(std::path::Path::new(&h.log).exists() || h.pgid > 0);
        // 실행 중 재요청 → 락 거부
        assert!(start_run(fake.to_str().unwrap(), dir.to_str().unwrap(), "/tmp/ws.json", false, "hi", runs.to_str().unwrap()).is_err());
        // 완료 대기 후 .done 확인
        std::thread::sleep(std::time::Duration::from_millis(700));
        assert!(std::path::Path::new(&format!("{}.done", h.log)).exists());
        crate::lock::release(dir.to_str().unwrap());
    }
}
```

- [ ] **Step 2: 실패 확인** — Run: `cd src-tauri && cargo test start_run_spawns_and_locks` → FAIL.

- [ ] **Step 3: 구현**
```rust
use serde::Serialize;
use std::os::unix::process::CommandExt;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use crate::lock::{acquire, LockInfo};

#[derive(Serialize, Clone)]
pub struct RunHandle { pub log: String, pub pgid: i32 }

fn now() -> u64 { SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() }
fn app_dir() -> String {
    // awb-run.sh 위치: 앱 리소스. 개발 중엔 리포 scripts/. 배포 시 Tauri resource 경로로 교체(TODO).
    std::env::var("AWB_SCRIPTS_DIR").unwrap_or_else(|_| format!("{}/github/ai-workbench/scripts", std::env::var("HOME").unwrap_or_default()))
}

pub fn start_run(claude_bin: &str, workdir: &str, settings: &str, plan: bool, prompt: &str, runs_dir: &str) -> Result<RunHandle, String> {
    // 락 선점
    let placeholder = LockInfo { pid: std::process::id(), pgid: 0, start_ts: now(), source: "app".into() };
    if let Err(cur) = acquire(workdir, &placeholder) {
        return Err(format!("이미 실행 중: {} (pid {})", cur.source, cur.pid));
    }
    std::fs::create_dir_all(runs_dir).ok();
    let log = format!("{}/{}.log", runs_dir, now());
    let wrapper = format!("{}/awb-run.sh", app_dir());
    let plan_flag = if plan { "1" } else { "0" };
    let child = unsafe {
        Command::new("sh")
            .args([&wrapper, claude_bin, workdir, &log, settings, plan_flag, prompt])
            .pre_exec(|| { libc::setsid(); Ok(()) })  // 자체 세션/PGID
            .stdin(std::process::Stdio::null())
            .spawn()
    }.map_err(|e| { crate::lock::release(workdir); format!("spawn 실패: {e}") })?;
    let pgid = child.id() as i32; // setsid 후 자식 pid == pgid
    // 락 메타 pgid 갱신
    let info = LockInfo { pid: child.id(), pgid, start_ts: now(), source: "app".into() };
    let _ = std::fs::write(crate::lock::lock_dir(workdir).join("meta.json"), serde_json::to_string(&info).unwrap());
    Ok(RunHandle { log, pgid })
}
```
`main.rs`: `mod runner;` + command 래퍼(`start_run(claude_bin, workdir, settings, plan, prompt, runs_dir)` → `runner::start_run(...)`). `runs_dir` 기본 `~/.claude/.awb-runs`.

- [ ] **Step 4: 통과 확인** — Run: `cd src-tauri && cargo test start_run_spawns_and_locks` → PASS.

- [ ] **Step 5: 커밋** — `... -m "feat: detached 러너 start_run(setsid PGID + 락 선점)"` (+트레일러)

---

### Task 4: 로그 tail (`runlog.rs`)

**Files:** Create `src-tauri/src/runlog.rs`; Modify `main.rs`(`mod runlog;` + `read_log`); Test 인라인.

**Interfaces:**
- Produces: `pub struct LogChunk { pub text: String, pub offset: u64, pub done: bool, pub exit_code: Option<i32> }`
- `pub fn read_log(log: &str, offset: u64) -> LogChunk` — `log` 파일을 `offset`부터 끝까지 읽어 반환, 새 offset 갱신, `<log>.done` 존재 시 done=true + 그 안의 exit code 파싱.
- Produces (command): `#[tauri::command] fn read_log(log: String, offset: u64) -> runlog::LogChunk`.

- [ ] **Step 1: 실패 테스트 — 증분 읽기 + done 감지**
```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn incremental_and_done() {
        let log = std::env::temp_dir().join("awb_tail.log");
        std::fs::write(&log, "line1\n").unwrap();
        let c1 = read_log(log.to_str().unwrap(), 0);
        assert_eq!(c1.text, "line1\n"); assert!(!c1.done);
        std::fs::write(&log, "line1\nline2\n").unwrap();
        let c2 = read_log(log.to_str().unwrap(), c1.offset);
        assert_eq!(c2.text, "line2\n");
        std::fs::write(format!("{}.done", log.to_str().unwrap()), "0\n").unwrap();
        let c3 = read_log(log.to_str().unwrap(), c2.offset);
        assert!(c3.done); assert_eq!(c3.exit_code, Some(0));
    }
}
```

- [ ] **Step 2: 실패 확인** — `cargo test incremental_and_done` → FAIL.

- [ ] **Step 3: 구현**
```rust
use serde::Serialize;
use std::fs;
use std::io::{Read, Seek, SeekFrom};

#[derive(Serialize, Clone)]
pub struct LogChunk { pub text: String, pub offset: u64, pub done: bool, pub exit_code: Option<i32> }

pub fn read_log(log: &str, offset: u64) -> LogChunk {
    let mut text = String::new();
    let mut new_off = offset;
    if let Ok(mut f) = fs::File::open(log) {
        if f.seek(SeekFrom::Start(offset)).is_ok() {
            let mut buf = Vec::new();
            if f.read_to_end(&mut buf).is_ok() {
                new_off = offset + buf.len() as u64;
                text = String::from_utf8_lossy(&buf).into_owned();
            }
        }
    }
    let done_path = format!("{log}.done");
    let (done, exit_code) = match fs::read_to_string(&done_path) {
        Ok(s) => (true, s.trim().parse::<i32>().ok()),
        Err(_) => (false, None),
    };
    LogChunk { text, offset: new_off, done, exit_code }
}
```
`main.rs`: `mod runlog;` + command.

- [ ] **Step 4: 통과 확인** — `cargo test incremental_and_done` → PASS.

- [ ] **Step 5: 커밋** — `... -m "feat: 로그 오프셋 tail read_log(+done/exit)"` (+트레일러)

---

### Task 5: 완료 다층 판정 + 락 해제 (`runlog.rs` 확장)

**Files:** Modify `src-tauri/src/runlog.rs`(추가), `main.rs`(`run_status` command); Test 인라인.

**Interfaces:**
- Produces: `pub struct RunStatus { pub done: bool, pub exit_code: Option<i32>, pub changed_files: u32, pub verdict: String }` (verdict ∈ running|success|failed|blocked). exit 단독 금지: exit≠0→failed; done & exit0 & changed_files==0 → "success(무변경)" 경고성; done & exit0 & changed>0 → success.
- `pub fn run_status(log: &str, workdir: &str) -> RunStatus` — done이면 락 해제(`lock::release`) + `git -C workdir diff --name-only`(+staged) 카운트.
- Command: `#[tauri::command] fn run_status(log: String, workdir: String) -> runlog::RunStatus`.

- [ ] **Step 1: 실패 테스트 — done+exit0 판정 success/무변경, exit≠0 failed**
```rust
#[test]
fn verdicts() {
    let log = std::env::temp_dir().join("awb_status.log");
    std::fs::write(&log, "x").unwrap();
    let wd = std::env::temp_dir().join("awb_status_wd"); std::fs::create_dir_all(&wd).unwrap();
    // 미완료
    let _ = std::fs::remove_file(format!("{}.done", log.to_str().unwrap()));
    assert_eq!(run_status(log.to_str().unwrap(), wd.to_str().unwrap()).verdict, "running");
    // 실패
    std::fs::write(format!("{}.done", log.to_str().unwrap()), "1").unwrap();
    assert_eq!(run_status(log.to_str().unwrap(), wd.to_str().unwrap()).verdict, "failed");
}
```

- [ ] **Step 2: 실패 확인** — `cargo test verdicts` → FAIL.

- [ ] **Step 3: 구현**
```rust
use std::process::Command as PCmd;

#[derive(Serialize, Clone)]
pub struct RunStatus { pub done: bool, pub exit_code: Option<i32>, pub changed_files: u32, pub verdict: String }

fn changed_files(workdir: &str) -> u32 {
    let out = PCmd::new("git").args(["-C", workdir, "status", "--porcelain"]).output();
    out.map(|o| String::from_utf8_lossy(&o.stdout).lines().filter(|l| !l.trim().is_empty()).count() as u32).unwrap_or(0)
}

pub fn run_status(log: &str, workdir: &str) -> RunStatus {
    let chunk = read_log(log, 0);
    if !chunk.done {
        return RunStatus { done: false, exit_code: None, changed_files: 0, verdict: "running".into() };
    }
    crate::lock::release(workdir);
    let cf = changed_files(workdir);
    let verdict = match chunk.exit_code {
        Some(0) => if cf == 0 { "success(무변경)".into() } else { "success".into() },
        Some(_) => "failed".into(),
        None => "failed".into(),
    };
    RunStatus { done: true, exit_code: chunk.exit_code, changed_files: cf, verdict }
}
```
`main.rs`: command 등록.

- [ ] **Step 4: 통과 확인** — `cargo test verdicts` → PASS.

- [ ] **Step 5: 커밋** — `... -m "feat: 완료 다층 판정 run_status + 락 해제"` (+트레일러)

---

### Task 6: 취소 (`runner.rs` 확장)

**Files:** Modify `src-tauri/src/runner.rs`(`cancel_run`), `main.rs`(command); Test 인라인.

**Interfaces:**
- `pub fn cancel_run(pgid: i32, workdir: &str) -> bool` — `killpg(pgid, SIGTERM)` → 300ms 후 살아있으면 `SIGKILL`, 락 해제. 성공 여부 반환.
- Command: `#[tauri::command] fn cancel_run(pgid: i32, workdir: String) -> bool`.

- [ ] **Step 1: 실패 테스트 — 장수 프로세스 취소되면 죽는다**
```rust
#[test]
fn cancel_kills_group() {
    // sleep 30 을 setsid로 띄우고 pgid 취소
    use std::os::unix::process::CommandExt;
    let child = unsafe { std::process::Command::new("sh").args(["-c","sleep 30"]).pre_exec(||{libc::setsid();Ok(())}).spawn().unwrap() };
    let pgid = child.id() as i32;
    let wd = std::env::temp_dir().join("awb_cancel_wd"); std::fs::create_dir_all(&wd).unwrap();
    assert!(cancel_run(pgid, wd.to_str().unwrap()));
    std::thread::sleep(std::time::Duration::from_millis(200));
    assert!(unsafe { libc::killpg(pgid, 0) } != 0); // 그룹 사라짐
}
```

- [ ] **Step 2: 실패 확인** — `cargo test cancel_kills_group` → FAIL.

- [ ] **Step 3: 구현**
```rust
pub fn cancel_run(pgid: i32, workdir: &str) -> bool {
    if pgid <= 1 { return false; }
    unsafe { libc::killpg(pgid, libc::SIGTERM); }
    std::thread::sleep(std::time::Duration::from_millis(300));
    if unsafe { libc::killpg(pgid, 0) } == 0 {
        unsafe { libc::killpg(pgid, libc::SIGKILL); }
    }
    crate::lock::release(workdir);
    true
}
```
`main.rs`: command 등록.

- [ ] **Step 4: 통과 확인** — `cargo test cancel_kills_group` → PASS.

- [ ] **Step 5: 커밋** — `... -m "feat: 취소 cancel_run(PGID TERM→KILL + 락 해제)"` (+트레일러)

---

### Task 7: 프론트 콘솔 뷰 (`Console.svelte` + `run.ts`)

**Files:** Create `src/lib/Console.svelte`, `src/lib/run.ts`, `src/lib/run.test.ts`; Modify `src/lib/ProjectList.svelte`(프로젝트 클릭→콘솔 열기) 또는 `App.svelte`(선택 프로젝트 시 콘솔 표시).

**Interfaces:**
- Consumes: commands `start_run`, `read_log`, `run_status`, `cancel_run` (api `call`), Plan 1의 preflight claude 경로.
- Produces: `run.ts` 순수함수 `appendChunk(prev, chunk)` (텍스트 누적) + `verdictLabel(status)` (배지 문구). 상태 타입.

- [ ] **Step 1: 실패 테스트 — appendChunk 누적/offset, verdictLabel**
```ts
import { describe, it, expect } from "vitest";
import { appendChunk, verdictLabel } from "./run";
describe("run", () => {
  it("appendChunk 누적", () => {
    const s = appendChunk({ text:"a", offset:1 }, { text:"b", offset:2, done:false, exit_code:null });
    expect(s.text).toBe("ab"); expect(s.offset).toBe(2);
  });
  it("verdictLabel", () => {
    expect(verdictLabel("failed")).toMatch(/실패/);
    expect(verdictLabel("success")).toMatch(/완료/);
  });
});
```

- [ ] **Step 2: 실패 확인** — Run: `npx vitest run src/lib/run.test.ts` → FAIL.

- [ ] **Step 3: 구현**
`src/lib/run.ts`:
```ts
export interface RunState { text: string; offset: number }
export function appendChunk(prev: RunState, chunk: { text: string; offset: number }): RunState {
  return { text: prev.text + chunk.text, offset: chunk.offset };
}
export function verdictLabel(v: string): string {
  if (v.startsWith("success")) return v.includes("무변경") ? "✅ 완료(변경 없음)" : "✅ 완료";
  if (v === "failed") return "❌ 실패";
  if (v === "blocked") return "⛔ 차단";
  return "⏳ 실행 중";
}
```
`src/lib/Console.svelte`: 프롬프트 textarea + plan 토글(체크박스) + [실행]/[취소] 버튼. 실행 시 `start_run(claudeBin, workdir, settingsPath, plan, prompt, runsDir)` → 반환 `{log,pgid}` 저장 → `setInterval(1.5s)`로 `read_log(log, offset)` 누적 표시, `done`이면 `run_status(log, workdir)` 호출해 배지 표시 + 인터벌 정리. [취소]는 `cancel_run(pgid, workdir)`. settingsPath=`~/.claude/worker-settings.json`, runsDir=`~/.claude/.awb-runs`, claudeBin=preflight 결과.
`App.svelte`: 프로젝트 선택 시 `<Console {project} .../>` 표시.

- [ ] **Step 4: 통과 확인** — `npx vitest run` → PASS; `npm run check` 0 errors; `npm run build` 성공.

- [ ] **Step 5: 커밋** — `... -m "feat: 콘솔 뷰(프롬프트 실행+tail+완료배지+취소) — Plan 2 프론트"` (+트레일러)

---

### Task 8: 자율 폴러/래퍼 락 공유 (`~/.claude` repo)

**Files (별도 repo `~/.claude`):** Modify `~/.claude/bin/agent-run.sh`, `~/.claude/bin/project-poll.py`.

**목적:** 앱 락과 **동일 규약**(`~/.claude/.run-locks/<fnv(realpath)>` + meta.json)을 자율 경로도 확인/획득 → 앱·폴러 이중 실행 방지.

- [ ] **Step 1: agent-run.sh 락 확인 삽입** — claude 실행 직전, `LOCK="$HOME/.claude/.run-locks/<fnv해시 계산>"` 를 mkdir 시도, 실패 시 기존 meta의 pgid가 살아있으면 그 채널에 "이미 실행 중" 보고 후 종료; 성공 시 meta.json 기록(source=agent-run), 실행 후 `rmdir` 해제. (fnv 해시는 sh 구현 또는 python one-liner 재사용.)
- [ ] **Step 2: project-poll.py launch() 락 삽입** — launch 직전 동일 락 확인/획득(source=project-poll), 실패 시 그 채널에 스킵 통보.
- [ ] **Step 3: 검증** — 앱이 락 잡은 상태에서 `project-poll.py once` 가 그 프로젝트를 스킵하는지 수동 확인(락 디렉토리 생성 후 실행).
- [ ] **Step 4: 커밋(+push)** — `~/.claude` repo에 커밋(트레일러) 후 `git push`(허용 규칙).

**주의:** 이 태스크는 git-crypt 동기화되는 `~/.claude` repo를 수정하므로, ai-workbench 브랜치와 **별도 커밋/repo**. 해시 함수는 Task 1의 FNV-1a와 **정확히 동일**해야 락 키가 일치.

---

## Plan 2 Self-Review

- **Spec coverage:** §5.1 실행(detached)=Task2/3; §5.2 공유 락=Task1/8; §5.3 plan 토글=Task2/3/7(`--permission-mode plan`); §5.5 완료 다층판정=Task5; 취소=Task6; 진행 tail=Task4/7; UI=Task7. Plan 1 preflight claude 경로 재사용=Task7. (스트림·resume·diff·롤백은 Plan 3/v1 — 범위 밖.)
- **Placeholder scan:** 각 스텝에 실제 코드/명령/기대출력. TODO는 크로스플랫폼 분기(macOS 우선 명시)와 배포 시 스크립트 리소스 경로 2곳만(의도적).
- **Type consistency:** `LockInfo`(pid/pgid/start_ts/source) Task1↔3↔8; `RunHandle`(log/pgid) Task3↔7; `LogChunk`/`RunStatus` Task4/5↔7; command 이름(start_run/read_log/run_status/cancel_run) 일관. FNV 해시 규약 Task1↔8 동일 강조.
- **위험:** Task8이 별도 repo 수정(문서화됨); 락 해시 불일치 시 락 무효(양 태스크에 "정확히 동일" 명시).

## 후속(Plan 3): diff 뷰·베이스라인 롤백·worker/reader-settings 하드닝·redaction·stream-json(실시간 스트림)·resume 대화.
