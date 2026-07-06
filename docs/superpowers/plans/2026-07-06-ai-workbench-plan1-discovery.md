# ai-workbench Plan 1 — Shell & Discovery 구현 계획

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Tauri 데스크톱 앱이 실행되어, 환경 프리플라이트를 수행하고, 설정된 루트에서 git 프로젝트를 발견해 worklog 배지와 함께 목록으로 보여준다.

**Architecture:** 얇은 Rust 코어(Tauri command)가 파일시스템·git·환경을 조사하고, Svelte 웹 UI가 결과를 렌더한다. 실행/디프 등은 후속 플랜(2~4).

**Tech Stack:** Tauri v2, Rust(stable), Svelte + Vite(SPA, SvelteKit 아님), Node ≥ 20, 테스트: Rust `#[test]` + `cargo test`, 프론트 로직은 vitest.

## Global Constraints

- Tauri **v2** (v1 아님). Node **≥ 20**. Rust **stable**. macOS 우선(유니버설), 스택은 크로스플랫폼 유지.
- claude CLI 경로는 **런타임 resolve**(하드코딩 금지) — 로그인 셸 PATH + 설정 오버라이드. 현재 실측 경로 `~/.local/bin/claude`는 기본 후보일 뿐.
- 기존 `~/.claude` 자산 재사용(worklog·worker-settings). 앱 저장소에 **시크릿·토큰 금지**.
- 프로젝트 인정 기준: `git rev-parse --show-toplevel == 그 폴더` **그리고** `origin` remote 존재.
- app state 경로: `~/.claude/app/<pc>/state.json` (Plan 4에서 영속화; Plan 1은 읽기 기본값 + 인메모리).
- 커밋 메시지 말미: `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`

## File Structure (Plan 1 범위)

```
ai-workbench/
  src-tauri/
    Cargo.toml                  Rust 의존성
    tauri.conf.json             Tauri 설정(창/식별자)
    src/
      main.rs                   Tauri 부트스트랩 + command 등록
      scan.rs                   project_scan: 루트→git repo 발견/정렬
      preflight.rs              preflight: claude/roots/git-crypt 점검
      shell_env.rs              로그인 셸 PATH 추출
      worklog.rs                worklog md 파싱→프로젝트 배지
  src/                          Svelte UI
    main.ts                     앱 진입
    App.svelte                  레이아웃(배너 + 목록)
    lib/api.ts                  Tauri invoke 래퍼(타입)
    lib/ProjectList.svelte      목록/검색/핀
    lib/PreflightBanner.svelte  프리플라이트 결과 배너
    lib/scan.test.ts            프론트 정렬/필터 유닛(vitest)
  package.json / vite.config.ts / tsconfig.json
```

책임: `scan.rs`=발견만, `preflight.rs`=환경판정만, `shell_env.rs`=PATH만, `worklog.rs`=배지만. 각 파일 단일 책임.

---

### Task 1: Tauri v2 + Svelte 스캐폴드 + command 왕복

**Files:**
- Create: `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, `src-tauri/src/main.rs`
- Create: `package.json`, `vite.config.ts`, `src/main.ts`, `src/App.svelte`, `src/lib/api.ts`
- Test: `src-tauri/src/main.rs` (`#[cfg(test)]` 인라인)

**Interfaces:**
- Produces: Tauri command `ping() -> String` (반환 `"pong"`); 프론트 `invoke<T>(cmd,args)` 래퍼.

- [ ] **Step 1: 스캐폴드 생성**

```bash
cd ~/github/ai-workbench
npm create tauri-app@latest . -- --template svelte-ts --manager npm --identifier com.parkmanhee.ai-workbench --yes
npm install
```
Expected: `src-tauri/`, `src/` 생성, `npm run tauri dev` 가능 상태.

- [ ] **Step 2: 실패 테스트 작성 — ping command**

`src-tauri/src/main.rs` 하단에 추가:
```rust
#[cfg(test)]
mod tests {
    #[test]
    fn ping_returns_pong() {
        assert_eq!(super::ping(), "pong");
    }
}
```

- [ ] **Step 3: 테스트 실패 확인**

Run: `cd src-tauri && cargo test ping_returns_pong`
Expected: FAIL — `cannot find function 'ping'`.

- [ ] **Step 4: 최소 구현**

`src-tauri/src/main.rs` 에 command 추가 + 등록:
```rust
#[tauri::command]
fn ping() -> String { "pong".to_string() }

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![ping])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

- [ ] **Step 5: 테스트 통과 확인**

Run: `cd src-tauri && cargo test ping_returns_pong`
Expected: PASS.

- [ ] **Step 6: 프론트 invoke 래퍼**

`src/lib/api.ts`:
```ts
import { invoke } from "@tauri-apps/api/core";
export function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  return invoke<T>(cmd, args);
}
```
`src/App.svelte` 최소 렌더에서 `call<string>("ping")` 결과를 표시(수동 확인용).

- [ ] **Step 7: 커밋**

```bash
git add -A
git commit -m "feat: Tauri v2 + Svelte 스캐폴드 + ping command

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 2: 로그인 셸 PATH 추출 (`shell_env.rs`)

**Files:**
- Create: `src-tauri/src/shell_env.rs`
- Modify: `src-tauri/src/main.rs` (`mod shell_env;`)
- Test: `src-tauri/src/shell_env.rs` 인라인

**Interfaces:**
- Produces: `pub fn login_path() -> String` — 로그인 셸(`$SHELL -lic 'printf %s "$PATH"'`)로 얻은 PATH. 실패 시 기본 PATH(`/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:~/.local/bin` 확장) 폴백.
- Produces: `pub fn which_in(path: &str, bin: &str) -> Option<String>` — 주어진 PATH에서 실행파일 절대경로 탐색.

- [ ] **Step 1: 실패 테스트 — which_in 이 PATH에서 찾는다**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    #[test]
    fn which_in_finds_executable() {
        let dir = std::env::temp_dir().join("awb_which_test");
        let _ = fs::create_dir_all(&dir);
        let bin = dir.join("mytool");
        fs::write(&bin, "#!/bin/sh\n").unwrap();
        #[cfg(unix)] { use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap(); }
        let found = which_in(dir.to_str().unwrap(), "mytool");
        assert_eq!(found, Some(bin.to_string_lossy().to_string()));
        assert_eq!(which_in(dir.to_str().unwrap(), "nope"), None);
    }
}
```

- [ ] **Step 2: 실패 확인**

Run: `cd src-tauri && cargo test which_in_finds_executable`
Expected: FAIL — `shell_env` 모듈/함수 없음.

- [ ] **Step 3: 구현**

`src-tauri/src/shell_env.rs`:
```rust
use std::path::Path;
use std::process::Command;

pub fn login_path() -> String {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
    let out = Command::new(&shell).args(["-lic", "printf %s \"$PATH\""]).output();
    if let Ok(o) = out {
        let p = String::from_utf8_lossy(&o.stdout).trim().to_string();
        if !p.is_empty() { return p; }
    }
    let home = std::env::var("HOME").unwrap_or_default();
    format!("/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:{}/.local/bin", home)
}

pub fn which_in(path: &str, bin: &str) -> Option<String> {
    for dir in path.split(':').filter(|s| !s.is_empty()) {
        let cand = Path::new(dir).join(bin);
        if cand.is_file() { return Some(cand.to_string_lossy().to_string()); }
    }
    None
}
```
`main.rs` 에 `mod shell_env;` 추가.

- [ ] **Step 4: 통과 확인**

Run: `cd src-tauri && cargo test which_in_finds_executable`
Expected: PASS.

- [ ] **Step 5: 커밋**

```bash
git add -A && git commit -m "feat: 로그인 셸 PATH 추출 + which_in

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 3: 프로젝트 발견 (`scan.rs`)

**Files:**
- Create: `src-tauri/src/scan.rs`
- Modify: `src-tauri/src/main.rs` (`mod scan;` + command 등록)
- Test: `src-tauri/src/scan.rs` 인라인

**Interfaces:**
- Consumes: 없음(표준 라이브러리 + git CLI).
- Produces:
  - `pub struct Project { pub name: String, pub path: String, pub has_origin: bool, pub last_activity: u64 }`
  - `pub fn scan_roots(roots: &[String]) -> Vec<Project>` — 각 루트의 1-depth 하위 폴더 중 `is_git_repo_root(dir) == true` 인 것만, `last_activity`(디렉토리 mtime) 내림차순 정렬. `node_modules`·숨김폴더 제외.
  - `pub fn is_git_repo_root(dir: &str) -> bool` — `git -C dir rev-parse --show-toplevel` 결과가 `dir`(realpath 일치) **그리고** `git -C dir remote get-url origin` 성공.
- Produces (command): `#[tauri::command] fn list_projects(roots: Vec<String>) -> Vec<Project>`.

- [ ] **Step 1: 실패 테스트 — git repo 루트만 인정, 중첩/비repo 제외**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, process::Command};
    fn git(args: &[&str], cwd: &std::path::Path) {
        Command::new("git").args(args).current_dir(cwd).output().unwrap();
    }
    #[test]
    fn scan_only_returns_git_roots_with_origin() {
        let base = std::env::temp_dir().join("awb_scan_test");
        let _ = fs::remove_dir_all(&base);
        let root = base.join("roots"); fs::create_dir_all(&root).unwrap();
        // repo A (origin O)
        let a = root.join("alpha"); fs::create_dir_all(&a).unwrap();
        git(&["init","-q"], &a); git(&["remote","add","origin","https://x/alpha.git"], &a);
        // plain dir (no git) -> 제외
        fs::create_dir_all(root.join("plaindir")).unwrap();
        // repo without origin -> 제외
        let b = root.join("beta"); fs::create_dir_all(&b).unwrap(); git(&["init","-q"], &b);
        let names: Vec<String> = scan_roots(&[root.to_string_lossy().to_string()])
            .into_iter().map(|p| p.name).collect();
        assert!(names.contains(&"alpha".to_string()));
        assert!(!names.contains(&"plaindir".to_string()));
        assert!(!names.contains(&"beta".to_string()));
    }
}
```

- [ ] **Step 2: 실패 확인**

Run: `cd src-tauri && cargo test scan_only_returns_git_roots_with_origin`
Expected: FAIL — `scan` 모듈 없음.

- [ ] **Step 3: 구현**

`src-tauri/src/scan.rs`:
```rust
use serde::Serialize;
use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Serialize, Clone)]
pub struct Project { pub name: String, pub path: String, pub has_origin: bool, pub last_activity: u64 }

fn realpath(p: &str) -> String {
    fs::canonicalize(p).map(|x| x.to_string_lossy().to_string()).unwrap_or_else(|_| p.to_string())
}

pub fn is_git_repo_root(dir: &str) -> bool {
    let top = Command::new("git").args(["-C", dir, "rev-parse", "--show-toplevel"]).output();
    let is_root = matches!(&top, Ok(o) if o.status.success()
        && realpath(String::from_utf8_lossy(&o.stdout).trim()) == realpath(dir));
    if !is_root { return false; }
    Command::new("git").args(["-C", dir, "remote", "get-url", "origin"])
        .output().map(|o| o.status.success()).unwrap_or(false)
}

fn mtime(p: &std::path::Path) -> u64 {
    p.metadata().and_then(|m| m.modified()).ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0)
}

pub fn scan_roots(roots: &[String]) -> Vec<Project> {
    let mut out = Vec::new();
    for root in roots {
        let expanded = shellexpand_tilde(root);
        let entries = match fs::read_dir(&expanded) { Ok(e) => e, Err(_) => continue };
        for e in entries.flatten() {
            let path = e.path();
            if !path.is_dir() { continue; }
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with('.') || name == "node_modules" { continue; }
            let ps = path.to_string_lossy().to_string();
            if is_git_repo_root(&ps) {
                let has_origin = true; // is_git_repo_root 가 origin 보장
                out.push(Project { name, path: realpath(&ps), has_origin, last_activity: mtime(&path) });
            }
        }
    }
    out.sort_by(|a, b| b.last_activity.cmp(&a.last_activity));
    out
}

fn shellexpand_tilde(p: &str) -> String {
    if let Some(rest) = p.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") { return format!("{}/{}", home, rest); }
    }
    p.to_string()
}
```
`main.rs`: `mod scan;` + command:
```rust
#[tauri::command]
fn list_projects(roots: Vec<String>) -> Vec<scan::Project> { scan::scan_roots(&roots) }
```
`serde` 를 `Cargo.toml`에 추가(`serde = { version = "1", features = ["derive"] }`).

- [ ] **Step 4: 통과 확인**

Run: `cd src-tauri && cargo test scan_only_returns_git_roots_with_origin`
Expected: PASS.

- [ ] **Step 5: command 등록 + 커밋**

`generate_handler![ping, list_projects]` 로 갱신 후:
```bash
git add -A && git commit -m "feat: project_scan — 루트에서 origin 있는 git repo 발견/정렬

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 4: 프리플라이트 (`preflight.rs`)

**Files:**
- Create: `src-tauri/src/preflight.rs`
- Modify: `src-tauri/src/main.rs` (`mod preflight;` + command)
- Test: `src-tauri/src/preflight.rs` 인라인

**Interfaces:**
- Consumes: `shell_env::login_path`, `shell_env::which_in`.
- Produces:
  - `pub struct Check { pub id: String, pub ok: bool, pub detail: String }`
  - `pub struct Preflight { pub claude_path: Option<String>, pub checks: Vec<Check> }`
  - `pub fn run_preflight(roots: &[String], claude_override: Option<String>) -> Preflight` — 점검: (1) claude 경로 resolve(override→login_path의 which_in→기본후보) + `--version` 성공, (2) roots 중 존재하는 게 1개 이상, (3) `~/.claude/worker-settings.json` 존재, (4) git-crypt 언락(대표 jsonl 첫 16바이트 `GITCRYPT` 아니면 ok=언락됨/평문, 있으면 잠김).
- Produces (command): `#[tauri::command] fn preflight(roots: Vec<String>, claude_override: Option<String>) -> Preflight`.

- [ ] **Step 1: 실패 테스트 — claude 없으면 claude 체크 실패, roots 없으면 roots 체크 실패**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preflight_flags_missing_claude_and_roots() {
        // 존재하지 않는 override + 빈 roots
        let pf = run_preflight(&[], Some("/no/such/claude".into()));
        let claude = pf.checks.iter().find(|c| c.id == "claude").unwrap();
        assert!(!claude.ok);
        let roots = pf.checks.iter().find(|c| c.id == "roots").unwrap();
        assert!(!roots.ok);
    }
}
```

- [ ] **Step 2: 실패 확인**

Run: `cd src-tauri && cargo test preflight_flags_missing_claude_and_roots`
Expected: FAIL — 모듈 없음.

- [ ] **Step 3: 구현**

`src-tauri/src/preflight.rs`:
```rust
use serde::Serialize;
use std::fs;
use std::process::Command;
use crate::shell_env::{login_path, which_in};

#[derive(Serialize, Clone)]
pub struct Check { pub id: String, pub ok: bool, pub detail: String }
#[derive(Serialize, Clone)]
pub struct Preflight { pub claude_path: Option<String>, pub checks: Vec<Check> }

fn home() -> String { std::env::var("HOME").unwrap_or_default() }

fn resolve_claude(override_path: Option<String>) -> Option<String> {
    if let Some(p) = override_path { if std::path::Path::new(&p).is_file() { return Some(p); } else { return None; } }
    if let Some(p) = which_in(&login_path(), "claude") { return Some(p); }
    let cand = format!("{}/.local/bin/claude", home());
    if std::path::Path::new(&cand).is_file() { Some(cand) } else { None }
}

pub fn run_preflight(roots: &[String], claude_override: Option<String>) -> Preflight {
    let mut checks = Vec::new();
    // 1. claude
    let claude_path = resolve_claude(claude_override.clone());
    let claude_ok = claude_path.as_ref().map(|p|
        Command::new(p).arg("--version").output().map(|o| o.status.success()).unwrap_or(false)
    ).unwrap_or(false);
    checks.push(Check { id: "claude".into(), ok: claude_ok,
        detail: claude_path.clone().unwrap_or_else(|| "claude 미발견 (PATH/설정 확인)".into()) });
    // 2. roots
    let roots_ok = roots.iter().any(|r| {
        let e = r.replace("~", &home()); std::path::Path::new(&e).is_dir() });
    checks.push(Check { id: "roots".into(), ok: roots_ok,
        detail: if roots_ok {"루트 유효".into()} else {"유효한 project_roots 없음".into()} });
    // 3. worker-settings
    let ws = format!("{}/.claude/worker-settings.json", home());
    let ws_ok = std::path::Path::new(&ws).is_file();
    checks.push(Check { id: "worker_settings".into(), ok: ws_ok, detail: ws });
    // 4. git-crypt unlock (대표 jsonl 매직)
    let locked = sample_locked();
    checks.push(Check { id: "git_crypt".into(), ok: !locked,
        detail: if locked {"트랜스크립트 잠김 — git-crypt unlock 필요".into()} else {"언락됨/평문".into()} });
    Preflight { claude_path, checks }
}

fn sample_locked() -> bool {
    let dir = format!("{}/.claude/projects", home());
    let walk = fs::read_dir(&dir);
    if let Ok(entries) = walk {
        for e in entries.flatten() {
            // 하위의 첫 .jsonl 하나만 샘플
            if let Ok(sub) = fs::read_dir(e.path()) {
                for f in sub.flatten() {
                    let p = f.path();
                    if p.extension().and_then(|s| s.to_str()) == Some("jsonl") {
                        if let Ok(bytes) = fs::read(&p) {
                            return bytes.windows(8).take(16).any(|w| w == b"GITCRYPT");
                        }
                    }
                }
            }
        }
    }
    false
}
```
`main.rs`: `mod preflight;` + `mod shell_env;`(이미) + command 등록.

- [ ] **Step 4: 통과 확인**

Run: `cd src-tauri && cargo test preflight_flags_missing_claude_and_roots`
Expected: PASS.

- [ ] **Step 5: 커밋**

```bash
git add -A && git commit -m "feat: preflight — claude/roots/worker-settings/git-crypt 점검

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 5: worklog 오버레이 (`worklog.rs`)

**Files:**
- Create: `src-tauri/src/worklog.rs`
- Modify: `src-tauri/src/main.rs` (`mod worklog;` + command)
- Test: `src-tauri/src/worklog.rs` 인라인

**Interfaces:**
- Produces:
  - `pub struct Badge { pub todo: u32, pub doing: u32, pub done: u32, pub updated: String }`
  - `pub fn badge_for(project_name: &str) -> Option<Badge>` — `~/.claude/worklog/<최신분기>/<project_name>.md` 를 basename 매핑으로 찾아 `⬜/🔄/✅` 섹션 항목 수와 "최종 갱신" 파싱. 없으면 None.
- Produces (command): `#[tauri::command] fn worklog_badge(name: String) -> Option<worklog::Badge>`.

- [ ] **Step 1: 실패 테스트 — 섹션 항목 카운트**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn counts_sections() {
        let md = "최종 갱신: 2026-07-06\n## ⬜ 해야 할 일\n- [ ] a\n- [ ] b\n## 🔄 진행 중\n- x\n## ✅ 한 일\n- y\n- z\n- w\n";
        let b = parse_badge(md);
        assert_eq!((b.todo, b.doing, b.done), (2,1,3));
        assert_eq!(b.updated, "2026-07-06");
    }
}
```

- [ ] **Step 2: 실패 확인**

Run: `cd src-tauri && cargo test counts_sections`
Expected: FAIL — 모듈 없음.

- [ ] **Step 3: 구현**

`src-tauri/src/worklog.rs`:
```rust
use serde::Serialize;
use std::fs;

#[derive(Serialize, Clone)]
pub struct Badge { pub todo: u32, pub doing: u32, pub done: u32, pub updated: String }

pub fn parse_badge(md: &str) -> Badge {
    let mut section = "";
    let (mut todo, mut doing, mut done) = (0u32, 0u32, 0u32);
    let mut updated = String::new();
    for line in md.lines() {
        let l = line.trim();
        if let Some(rest) = l.strip_prefix("최종 갱신:") { updated = rest.trim().to_string(); }
        if l.starts_with("## ") {
            section = if l.contains("해야 할 일") {"todo"}
                else if l.contains("진행 중") {"doing"}
                else if l.contains("한 일") {"done"} else {""};
            continue;
        }
        let is_item = l.starts_with("- ");
        if !is_item { continue; }
        let body = l.trim_start_matches("- ").trim_start_matches("[ ]").trim_start_matches("[x]").trim();
        if body.is_empty() || body.starts_with('(') { continue; } // 플레이스홀더 제외
        match section { "todo"=>todo+=1, "doing"=>doing+=1, "done"=>done+=1, _=>{} }
    }
    Badge { todo, doing, done, updated }
}

fn home() -> String { std::env::var("HOME").unwrap_or_default() }

pub fn badge_for(project_name: &str) -> Option<Badge> {
    let base = format!("{}/.claude/worklog", home());
    // 최신 분기 디렉토리(내림차순 첫번째)
    let mut quarters: Vec<String> = fs::read_dir(&base).ok()?
        .flatten().filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.contains("-Q")).collect();
    quarters.sort(); quarters.reverse();
    for q in quarters {
        let p = format!("{}/{}/{}.md", base, q, project_name);
        if let Ok(md) = fs::read_to_string(&p) { return Some(parse_badge(&md)); }
    }
    None
}
```
`main.rs`: `mod worklog;` + command 등록.

- [ ] **Step 4: 통과 확인**

Run: `cd src-tauri && cargo test counts_sections`
Expected: PASS.

- [ ] **Step 5: 커밋**

```bash
git add -A && git commit -m "feat: worklog 배지 파싱/매핑

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 6: 프론트 목록 UI + 프리플라이트 배너

**Files:**
- Modify: `src/App.svelte`
- Create: `src/lib/ProjectList.svelte`, `src/lib/PreflightBanner.svelte`, `src/lib/types.ts`
- Test: `src/lib/scan.test.ts` (정렬/필터 순수함수)

**Interfaces:**
- Consumes: command `list_projects(roots)`, `preflight(roots, claude_override)`, `worklog_badge(name)` (api.ts `call`).
- Produces: 순수함수 `filterProjects(list, query)` (이름 부분일치, 대소문자 무시), `src/lib/scan.ts` 에.

- [ ] **Step 1: 실패 테스트 — filterProjects**

`src/lib/scan.test.ts`:
```ts
import { describe, it, expect } from "vitest";
import { filterProjects } from "./scan";
describe("filterProjects", () => {
  it("이름 부분일치·대소문자 무시", () => {
    const list = [{name:"csms-api"},{name:"Java-OCA"}] as any;
    expect(filterProjects(list,"oca").map(p=>p.name)).toEqual(["Java-OCA"]);
    expect(filterProjects(list,"").length).toBe(2);
  });
});
```

- [ ] **Step 2: 실패 확인**

Run: `npx vitest run src/lib/scan.test.ts`
Expected: FAIL — `filterProjects`/모듈 없음. (vitest 미설치면 `npm i -D vitest` 후 재실행.)

- [ ] **Step 3: 구현 — 순수함수 + 컴포넌트**

`src/lib/scan.ts`:
```ts
export interface Project { name: string; path: string; has_origin: boolean; last_activity: number }
export function filterProjects<T extends {name:string}>(list: T[], q: string): T[] {
  const s = q.trim().toLowerCase();
  return s ? list.filter(p => p.name.toLowerCase().includes(s)) : list;
}
```
`src/lib/ProjectList.svelte`: `list_projects` 호출→`filterProjects`로 필터→각 항목에 `worklog_badge` 배지. 검색 인풋 + 핀(로컬 배열, Plan 4에서 영속).
`src/lib/PreflightBanner.svelte`: `preflight` 호출→실패 체크만 빨간 배너로 나열(각 detail + 해결 힌트). 모두 통과면 숨김.
`src/App.svelte`: `<PreflightBanner/>` + `<ProjectList/>`. 기본 roots=`["~/bitbucket","~/github"]`(Plan 4에서 설정화).

- [ ] **Step 4: 통과 확인**

Run: `npx vitest run src/lib/scan.test.ts`
Expected: PASS.

- [ ] **Step 5: 수동 스모크 — 앱 실행**

Run: `npm run tauri dev`
Expected: 창이 뜨고, `~/bitbucket`의 repo들이 목록으로(최근순) 보이며, worklog 있는 프로젝트에 ⬜/🔄/✅ 배지. claude 미발견 등 문제 시 상단 배너.

- [ ] **Step 6: 커밋**

```bash
git add -A && git commit -m "feat: 프로젝트 목록 UI + 프리플라이트 배너 (Plan 1 완료)

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Plan 1 Self-Review

- **Spec coverage(Plan 1 부분):** §3 프로젝트목록(설정형 roots·git한정·정렬·worklog)=Task3/5/6 ✅; §5.3 프리플라이트(claude/PATH/roots/git-crypt)=Task2/4 ✅; §2 스택(Tauri v2+Svelte)=Task1 ✅. §4 실행·§6 diff·§7 상태영속·§8 sync는 **Plan 2~4**(의도적 범위 밖).
- **Placeholder scan:** 각 스텝에 실제 코드/명령/기대출력 포함, TODO 없음.
- **Type consistency:** `Project`(name/path/has_origin/last_activity) Task3↔Task6 일치, `Check`/`Preflight` Task4 일관, `Badge` Task5 일관, command 이름(list_projects/preflight/worklog_badge/ping) 일관.

---

## 후속 플랜 로드맵 (각각 독립 실행가능 소프트웨어)

- **Plan 2 — Execution & Progress:** 공유 실행 락(realpath, 앱+agent-run.sh+project-poll.py), detached runner(agent-run 패턴), 로그 tail(1~2s), 완료 다층판정, plan/실행 토글, 취소. → "프롬프트 실행하고 진행·완료 확인".
- **Plan 3 — Results & Safety:** `git diff --stat`+diff 뷰, 베이스라인 스냅샷 롤백, worker/reader-settings 하드닝(deny+시크릿차단+MCP락), 표시/저장 redaction. → "변경 검토·안전 롤백·강화된 권한".
- **Plan 4 — State & Sync:** app state 영속(PC별, 원자쓰기)·roots 설정 UI, git 신선도(sync.sh 경유), worklog 패널 주입, 핀 영속, 마무리. → "설정 지속·동기화 인지".

각 플랜은 Plan 1 완료 후 이 스킬로 상세화한다.
