# ai-workbench Mobile — Plan 1: awb-core lib 추출 구현 계획

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax.

**Goal:** 기존 데스크톱 앱의 순수 로직 8개 모듈(scan·runner·runlog·lock·preflight·paths·shell_env·worklog)을 Cargo 워크스페이스의 `crates/awb-core` lib 크레이트로 추출하고, `src-tauri` 앱이 이를 의존하도록 리와이어한다. 데몬(Plan 2)·데스크톱이 같은 코어를 공유하는 토대.

**Architecture:** 리포 루트에 Cargo 워크스페이스를 신설(`src-tauri` + `crates/awb-core`). 8개 모듈은 tauri 비의존 순수 로직이고 모듈 간 참조(`crate::lock`·`crate::paths`·`crate::shell_env`)가 전부 awb-core 내부로 유지되므로 무손실 이동 가능. `src-tauri/src/main.rs`의 `#[tauri::command]` 얇은 래퍼만 앱에 남고 `mod X` → `use awb_core::X`로 바뀐다. 기존 Rust `#[test]`가 그대로 따라 이동하여 회귀 안전망이 된다.

**Tech Stack:** Rust(stable, edition 2021), Cargo 워크스페이스(resolver 2), Tauri v2. 기존 의존성 그대로(serde/serde_json/libc). 테스트: 기존 `cargo test` 스위트.

## Global Constraints

- 커밋 트레일러(마지막 줄): `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`
- macOS 우선(유닉스 `setsid`/`killpg`); 크로스플랫폼 분기는 기존 주석 TODO 유지.
- **awb-core는 순수 로직만** — tauri·HTTP·전송 의존 금지(데몬/앱이 공유하는 경계).
- 모노레포 최종 배치: `crates/awb-core`, `crates/awb-server`(Plan 2), `src-tauri`, `mobile`(Plan 3).
- 공유 실행 락(runlock, FNV realpath 키)의 동작은 이 리팩터로 바뀌지 않음 — 코드 이동만.
- 리팩터 원칙: 기존 테스트는 **삭제·약화 금지**, 이동만. 이동 후 전부 green이어야 함.

## File Structure (Plan 1 범위)

```
Cargo.toml                     신규 — [workspace] 루트 (members = src-tauri, crates/awb-core)
.gitignore                     수정 — /target 추가(워크스페이스 빌드 출력이 루트로 이동)
crates/awb-core/
  Cargo.toml                   신규 — lib 크레이트(serde/serde_json/libc)
  src/lib.rs                   신규 — pub mod 선언(공개 API 표면)
  src/{lock,paths,preflight,runner,runlog,scan,shell_env,worklog}.rs   src-tauri/src에서 이동
src-tauri/
  Cargo.toml                   수정 — awb-core 경로 의존 추가
  src/main.rs                  수정 — mod 선언 제거 → use awb_core::{...}
```

---

### Task 1: Cargo 워크스페이스 + awb-core 스켈레톤

**Files:**
- Create: `Cargo.toml`(리포 루트), `crates/awb-core/Cargo.toml`, `crates/awb-core/src/lib.rs`
- Modify: `.gitignore`

**Interfaces:**
- Produces: 컴파일되는 빈 `awb_core` 크레이트(공개 심볼 아직 없음). 워크스페이스가 `src-tauri`를 기존과 동일하게 빌드.
- Consumes: 없음.

- [ ] **Step 1: 루트 워크스페이스 Cargo.toml 작성**

Create `Cargo.toml`:
```toml
[workspace]
resolver = "2"
members = ["src-tauri", "crates/awb-core"]
```

- [ ] **Step 2: awb-core 크레이트 매니페스트 작성**

Create `crates/awb-core/Cargo.toml`:
```toml
[package]
name = "awb-core"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
libc = "0.2"
```

- [ ] **Step 3: 빈 lib.rs 작성**

Create `crates/awb-core/src/lib.rs`:
```rust
//! ai-workbench 코어 로직 — 데스크톱 앱·데몬이 공유. 전송/UI 비의존 순수 로직.
```

- [ ] **Step 4: .gitignore에 루트 target 추가**

Modify `.gitignore` — 파일 끝에 한 줄 추가:
```
/target
```
(이유: 워크스페이스 신설로 빌드 출력이 `src-tauri/target`에서 리포 루트 `/target`으로 이동.)

- [ ] **Step 5: 워크스페이스 빌드 + 기존 테스트 green 확인**

Run: `cargo build --workspace`
Expected: 성공(경고만 허용, 에러 0). `src-tauri`가 워크스페이스 멤버로 그대로 빌드됨.

Run: `cargo test --workspace`
Expected: PASS — 기존 src-tauri 테스트(lock/runner/runlog + ping) 전부 통과, awb-core는 테스트 0개.

만약 Tauri가 워크스페이스에서 `target` 경로로 실패하면: 루트 `Cargo.toml`에 `resolver = "2"`가 있는지 확인(edition 2021 멤버 필수).

- [ ] **Step 6: 커밋**

```bash
git add Cargo.toml crates/awb-core/Cargo.toml crates/awb-core/src/lib.rs .gitignore
git commit -m "$(printf 'chore: Cargo 워크스페이스 + awb-core 스켈레톤 크레이트\n\nCo-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>')"
```

---

### Task 2: 8개 모듈을 awb-core로 이동 + src-tauri 리와이어

**Files:**
- Move (git mv): `src-tauri/src/{lock,paths,preflight,runner,runlog,scan,shell_env,worklog}.rs` → `crates/awb-core/src/`
- Modify: `crates/awb-core/src/lib.rs`, `src-tauri/Cargo.toml`, `src-tauri/src/main.rs`
- Test: 이동한 모듈 내 기존 인라인 `#[cfg(test)] mod tests`(lock/runner/runlog 등) — 그대로 따라 이동.

**Interfaces:**
- Consumes: Task 1의 `awb-core` 크레이트.
- Produces (공개 API, Plan 2 데몬이 소비):
  - `awb_core::scan::{Project, scan_roots, is_git_repo_root}`
  - `awb_core::runner::{RunHandle, start_run, cancel_run}` — `start_run(claude_bin:&str, workdir:&str, settings:&str, plan:bool, prompt:&str, runs_dir:&str) -> Result<RunHandle,String>`, `cancel_run(pgid:i32, workdir:&str) -> bool`
  - `awb_core::runlog::{LogChunk, RunStatus, read_log, run_status}` — `read_log(log:&str, offset:u64) -> LogChunk`, `run_status(log:&str, workdir:&str) -> RunStatus`
  - `awb_core::lock::{LockInfo, lock_dir, status, acquire, release}`
  - `awb_core::preflight::{Check, Preflight, run_preflight}`
  - `awb_core::paths::expand_tilde`
  - `awb_core::shell_env::{login_path, which_in}`
  - `awb_core::worklog::{Badge, parse_badge, badge_for}`

- [ ] **Step 1: 8개 모듈 파일 이동(git mv)**

```bash
cd /Users/mh/github/ai-workbench
git mv src-tauri/src/lock.rs      crates/awb-core/src/lock.rs
git mv src-tauri/src/paths.rs     crates/awb-core/src/paths.rs
git mv src-tauri/src/preflight.rs crates/awb-core/src/preflight.rs
git mv src-tauri/src/runner.rs    crates/awb-core/src/runner.rs
git mv src-tauri/src/runlog.rs    crates/awb-core/src/runlog.rs
git mv src-tauri/src/scan.rs      crates/awb-core/src/scan.rs
git mv src-tauri/src/shell_env.rs crates/awb-core/src/shell_env.rs
git mv src-tauri/src/worklog.rs   crates/awb-core/src/worklog.rs
```
모듈 내부의 `crate::lock`·`crate::paths`·`crate::shell_env` 참조는 **수정 불필요** — 이제 awb-core 안에서 여전히 `crate::`로 유효(같은 크레이트).

- [ ] **Step 2: lib.rs에 pub mod 선언**

Replace `crates/awb-core/src/lib.rs` 전체:
```rust
//! ai-workbench 코어 로직 — 데스크톱 앱·데몬이 공유. 전송/UI 비의존 순수 로직.

pub mod lock;
pub mod paths;
pub mod preflight;
pub mod runner;
pub mod runlog;
pub mod scan;
pub mod shell_env;
pub mod worklog;
```

- [ ] **Step 3: awb-core 단독 테스트가 green인지 확인**

Run: `cargo test -p awb-core`
Expected: PASS — lock(`acquire_is_exclusive_then_releasable`, stale-steal), runner(`start_run_spawns_and_locks`, `cancel_kills_group`), runlog(`incremental_and_done`, `verdicts`) 등 이동한 테스트가 새 크레이트에서 전부 통과.
(이 시점 `src-tauri`는 아직 `mod` 선언이 사라진 파일을 참조하지 못해 컴파일 실패 — Step 5까지 정상.)

- [ ] **Step 4: src-tauri가 awb-core를 의존하도록 매니페스트 수정**

Modify `src-tauri/Cargo.toml` — `[dependencies]` 섹션에 추가:
```toml
awb-core = { path = "../crates/awb-core" }
```
(기존 tauri/serde/serde_json/libc 라인은 유지. libc는 main.rs가 직접 쓰지 않으면 이후 정리 가능하나 이 태스크에선 건드리지 않음.)

- [ ] **Step 5: main.rs의 mod 선언을 use로 교체**

Modify `src-tauri/src/main.rs` — 4~11번째 줄의 `mod` 블록을 다음으로 교체:
```rust
use awb_core::{scan, preflight, worklog, runner, runlog};
```
(제거: `mod lock; mod paths; mod preflight; mod runner; mod runlog; mod scan; mod shell_env; mod worklog;`. `lock`·`paths`·`shell_env`는 main.rs가 직접 참조하지 않고 다른 모듈이 내부적으로만 쓰므로 use 목록에서 제외. 나머지 함수 본문의 `scan::`, `preflight::`, `worklog::`, `runner::`, `runlog::` 참조는 그대로 유효.)

- [ ] **Step 6: 워크스페이스 전체 테스트 green 확인**

Run: `cargo test --workspace`
Expected: PASS — awb-core 이동 테스트 전부 + src-tauri `ping_returns_pong` 통과. 에러 0.

Run: `cargo build --workspace`
Expected: 성공. `src-tauri/src`에는 `main.rs`만 남고 8개 모듈이 없어도 컴파일됨.

- [ ] **Step 7: 데스크톱 앱 런타임 스모크(리팩터가 동작을 안 깼는지)**

Run: `npm run tauri dev` (앱 창이 뜨면 프리플라이트 배너 표시 → 프로젝트 목록 로드 확인 → 창 닫기)
Expected: v0와 동일하게 프로젝트 목록·프리플라이트가 뜬다. (헤드리스 환경이면 이 스텝은 리뷰어가 수동 확인으로 대체; `cargo build`로 최소 보증.)

- [ ] **Step 8: 커밋**

```bash
git add -A
git commit -m "$(printf 'refactor: 순수 로직 8모듈을 awb-core lib로 추출 + src-tauri 리와이어\n\nscan/runner/runlog/lock/preflight/paths/shell_env/worklog 이동. 기존 테스트 무손실.\n데몬(Plan2)/데스크톱 공유 코어 토대.\n\nCo-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>')"
```

---

## Plan 1 Self-Review

- **Spec coverage:** 스펙 "awb-core (신규 lib): scan/runner/runlog/lock/preflight/paths ... lib 크레이트로 추출" = Task 1(스켈레톤)+Task 2(이동/리와이어). 모노레포 구조(`crates/awb-core`, `src-tauri`)=Task 1 워크스페이스. 서버/모바일/전송은 Plan 2·3 범위 — 이 플랜에 없음(의도적).
- **Placeholder scan:** 각 스텝에 실제 파일 경로·명령·기대출력·전체 코드 블록. TBD/TODO 없음. 크로스플랫폼 주석 TODO는 기존 코드 유지(신규 아님).
- **Type consistency:** Task 2 "Produces" 목록의 시그니처는 현재 소스에서 검증된 실제 pub 시그니처(`start_run` 6인자, `cancel_run(pgid,workdir)`, `read_log(log,offset)`, `run_status(log,workdir)`, `scan_roots(&[String])->Vec<Project>` 등)와 일치. main.rs use 목록(scan/preflight/worklog/runner/runlog)은 실제 참조 모듈과 일치, lock/paths/shell_env 제외도 근거 명시.
- **위험:** 워크스페이스 신설이 Tauri 빌드 경로(target 이동)에 영향 → `.gitignore /target`(Task1 Step4)와 `resolver="2"`로 대응. 이동 중간(Step3~4)의 src-tauri 컴파일 실패는 예상된 상태로 명시.

## 다음 플랜
- **Plan 2 (awb-server):** HTTP/WS 데몬·QR페어링/토큰·`claude --resume` 멀티턴·stream-json→WS 브릿지·cancel/status/diff·Expo 푸시·`/awake` 전원 어서션·서버 단일 인스턴스 가드. awb-core 공개 API 소비.
- **Plan 3 (mobile):** RN+Expo 앱 — 페어링/프로젝트목록/채팅·WS 스트리밍·푸시 등록·git요약. Plan 2 API 소비.
