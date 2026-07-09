# ai-workbench Mobile — Plan 4: PC↔폰 세션 연속성

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`) syntax.

**Goal:** 앱의 핵심 — 하나의 Claude 세션을 **PC↔폰 어디서든 이어서** 작업. 폰이 프로젝트의 과거/현재 세션 목록을 보고, 트랜스크립트(대화 내역)를 읽고, 특정 세션을 `--resume`으로 이어가며, 활성 세션은 실시간으로 지켜본다. 폰에서 진행한 세션은 PC가 `claude --resume <id>`로 이어받도록 앱이 그 명령을 안내한다.

**연속성 모델(확정):** **무봉제 핸드오프** — 세션 트랜스크립트(`~/.claude/projects/<slug>/<sessionId>.jsonl`)가 양쪽 공유 단일 진실원본. 한 기기가 진행할 때 다른 기기는 실시간으로 보고, 진행권을 넘겨받아 resume. 동시 조종은 불가(충돌)하므로 하지 않는다.

**Architecture:** 데몬(awb-server)이 `~/.claude/projects/<slug>` 를 스캔해 세션 목록·트랜스크립트를 제공(읽기)하고, `/chat`에 특정 sessionId resume를 지원. 폰은 세션 목록→트랜스크립트→이어하기 화면을 추가. 트랜스크립트 파싱은 **데몬에서** 수행해 메시지 DTO로 반환(폰은 렌더만).

**Tech Stack:** 기존 그대로 — Rust/axum(awb-server), awb-core, RN/Expo(mobile). 테스트: 데몬 `#[test]`(픽스처 jsonl), 폰 vitest(api).

## Global Constraints

- 커밋 트레일러(마지막 줄): `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`
- **읽기 전용 + resume만** — 데몬은 트랜스크립트를 읽기만, 쓰기·수정 안 함. 진행은 `/chat`(claude --resume)이 담당(기존 stream-json 경로 재사용).
- **프로젝트 슬러그 = workdir의 비영숫자 문자를 `-`로 치환** (예: `/Users/mh/github/ai-workbench` → `-Users-mh-github-ai-workbench`). Claude Code 규약과 일치(검증됨).
- **경로 traversal 방지**: 클라이언트가 준 sessionId는 `[A-Za-z0-9-]`만 허용(검증 실패 시 400).
- 모든 신규 엔드포인트는 Plan 2a `require_token` 뒤. 폰 변경은 JS(라이브 리로드), 데몬 변경은 Mac-side(USB 불필요).
- 트랜스크립트 라인 형식: `type` in {user, assistant}만 메시지; `message.content`가 문자열이면 텍스트, 배열이면 `text` 블록 연결 + `tool_use` 이름 수집. 그 외 type(meta/system 등) 무시.

## File Structure (Plan 4 범위)

```
crates/awb-server/src/
  transcript.rs   신규 — 슬러그 계산, 세션 목록(list_sessions), 트랜스크립트 파싱(read_transcript)
  routes.rs       수정 — /sessions/{project}, /transcript/{project}/{session_id} 핸들러 + /chat resume_session_id
mobile/src/lib/
  api.ts          수정 — sessions(), transcript(), chat()에 resumeSessionId 옵션
  types.ts        수정 — SessionInfo, TranscriptMsg 타입
mobile/app/
  sessions/[project].tsx   신규 — 세션 목록(+새 대화)
  chat/[project].tsx       수정 — session 파라미터로 트랜스크립트 로드+resume, [PC에서 이어받기] 명령 표시, 활성 세션 실시간 폴
```

---

### Task 1: 데몬 — 세션 목록 + 트랜스크립트 파싱 + 엔드포인트 + resume

**Files:** Create `crates/awb-server/src/transcript.rs`; Modify `main.rs`(`mod transcript;`), `routes.rs`(핸들러 3곳).

**Interfaces:**
- Produces: `transcript::project_slug(workdir: &str) -> String` (비영숫자→`-`).
- `transcript::SessionInfo { session_id, updated: u64, preview: String, count: u32, active: bool }` (serde Serialize).
- `transcript::list_sessions(slug_dir: &str) -> Vec<SessionInfo>` — `<projects>/<slug>/*.jsonl` 스캔, updated desc 정렬. active = mtime within 90s of now.
- `transcript::TranscriptMsg { role: String, text: String, tools: Vec<String> }` (serde Serialize).
- `transcript::read_transcript(path: &str, from_line: usize) -> (Vec<TranscriptMsg>, usize, bool)` — `from_line`부터 메시지 파싱, 새 line index + active 반환(라인 기반 offset으로 실시간 폴 지원).
- `transcript::safe_session_id(s: &str) -> bool` — `[A-Za-z0-9-]`만.
- Routes: `GET /sessions/{project}` → `Vec<SessionInfo>`; `GET /transcript/{project}/{session_id}?from=N` → `{messages, next: usize, active: bool}`; `POST /chat/{project}` body에 `resume_session_id: Option<String>` 추가(있으면 그 값으로 resume, 없으면 기존 세션스토어 값).

- [ ] **Step 1: 실패 테스트 — 슬러그/파싱/세션ID 검증**
```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn slug_matches_claude_convention() {
        assert_eq!(project_slug("/Users/mh/github/ai-workbench"), "-Users-mh-github-ai-workbench");
        assert_eq!(project_slug("/a/b.c_d"), "-a-b-c-d");
    }
    #[test]
    fn safe_session_id_rejects_traversal() {
        assert!(safe_session_id("0504bb6f-da3c-4c2d"));
        assert!(!safe_session_id("../etc/passwd"));
        assert!(!safe_session_id("a/b"));
    }
    #[test]
    fn parses_user_and_assistant_lines() {
        let dir = std::env::temp_dir().join("awb_tx"); std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("s1.jsonl");
        std::fs::write(&f, concat!(
            "{\"type\":\"mode\",\"sessionId\":\"s1\"}\n",
            "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"안녕\"}}\n",
            "{\"type\":\"assistant\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"반가워\"},{\"type\":\"tool_use\",\"name\":\"Bash\"}]}}\n"
        )).unwrap();
        let (msgs, next, _active) = read_transcript(f.to_str().unwrap(), 0);
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].role, "user"); assert_eq!(msgs[0].text, "안녕");
        assert_eq!(msgs[1].role, "assistant"); assert_eq!(msgs[1].text, "반가워");
        assert_eq!(msgs[1].tools, vec!["Bash".to_string()]);
        assert_eq!(next, 3); // 3 lines consumed
    }
}
```

- [ ] **Step 2: 실패 확인** — `cargo test -p awb-server transcript::` → FAIL.

- [ ] **Step 3: 구현 (transcript.rs)**
```rust
use serde::Serialize;
use serde_json::Value;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn project_slug(workdir: &str) -> String {
    workdir.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect()
}
pub fn safe_session_id(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}
fn now() -> u64 { SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) }
fn home() -> String { std::env::var("HOME").unwrap_or_default() }
fn projects_root() -> String { format!("{}/.claude/projects", home()) }

#[derive(Serialize, Clone)]
pub struct SessionInfo { pub session_id: String, pub updated: u64, pub preview: String, pub count: u32, pub active: bool }
#[derive(Serialize, Clone)]
pub struct TranscriptMsg { pub role: String, pub text: String, pub tools: Vec<String> }

fn parse_content(v: &Value) -> (String, Vec<String>) {
    match v {
        Value::String(s) => (s.clone(), vec![]),
        Value::Array(arr) => {
            let mut text = String::new(); let mut tools = vec![];
            for it in arr {
                match it.get("type").and_then(|t| t.as_str()) {
                    Some("text") => if let Some(t) = it.get("text").and_then(|x| x.as_str()) { text.push_str(t); },
                    Some("tool_use") => if let Some(n) = it.get("name").and_then(|x| x.as_str()) { tools.push(n.to_string()); },
                    _ => {}
                }
            }
            (text, tools)
        }
        _ => (String::new(), vec![]),
    }
}

pub fn read_transcript(path: &str, from_line: usize) -> (Vec<TranscriptMsg>, usize, bool) {
    let content = match fs::read_to_string(path) { Ok(c) => c, Err(_) => return (vec![], from_line, false) };
    let lines: Vec<&str> = content.lines().collect();
    let mut msgs = vec![];
    for line in lines.iter().skip(from_line) {
        let v: Value = match serde_json::from_str(line) { Ok(v) => v, Err(_) => continue };
        match v.get("type").and_then(|t| t.as_str()) {
            Some(r @ ("user" | "assistant")) => {
                if let Some(c) = v.get("message").and_then(|m| m.get("content")) {
                    let (text, tools) = parse_content(c);
                    if !text.is_empty() || !tools.is_empty() {
                        msgs.push(TranscriptMsg { role: r.to_string(), text, tools });
                    }
                }
            }
            _ => {}
        }
    }
    let active = fs::metadata(path).and_then(|m| m.modified()).ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| now().saturating_sub(d.as_secs()) <= 90).unwrap_or(false);
    (msgs, lines.len(), active)
}

pub fn list_sessions(slug: &str) -> Vec<SessionInfo> {
    let dir = format!("{}/{}", projects_root(), slug);
    let mut out = vec![];
    let entries = match fs::read_dir(&dir) { Ok(e) => e, Err(_) => return out };
    for e in entries.flatten() {
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()) != Some("jsonl") { continue; }
        let sid = p.file_stem().and_then(|x| x.to_str()).unwrap_or("").to_string();
        if !safe_session_id(&sid) { continue; }
        let updated = e.metadata().and_then(|m| m.modified()).ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0);
        let (msgs, _n, active) = read_transcript(p.to_str().unwrap_or(""), 0);
        let preview = msgs.iter().find(|m| m.role == "user").map(|m| {
            let t: String = m.text.chars().take(60).collect(); t
        }).unwrap_or_default();
        out.push(SessionInfo { session_id: sid, updated, preview, count: msgs.len() as u32, active });
    }
    out.sort_by(|a, b| b.updated.cmp(&a.updated));
    out
}
```
`main.rs`에 `mod transcript;`.

- [ ] **Step 4: 통과 확인** — `cargo test -p awb-server transcript::` → PASS.

- [ ] **Step 5: 라우트 배선 (routes.rs)** — 핸들러 추가(인증군):
```rust
pub async fn sessions_handler(State(st): State<AppState>, Path(project): Path<String>) -> Result<Json<Vec<crate::transcript::SessionInfo>>, StatusCode> {
    let proj = awb_core::scan::scan_roots(&st.roots).into_iter().find(|p| p.name == project).ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(crate::transcript::list_sessions(&crate::transcript::project_slug(&proj.path))))
}
#[derive(serde::Deserialize)]
pub struct TxQuery { #[serde(default)] pub from: usize }
pub async fn transcript_handler(State(st): State<AppState>, Path((project, session_id)): Path<(String, String)>, Query(q): Query<TxQuery>) -> Result<Json<serde_json::Value>, StatusCode> {
    if !crate::transcript::safe_session_id(&session_id) { return Err(StatusCode::BAD_REQUEST); }
    let proj = awb_core::scan::scan_roots(&st.roots).into_iter().find(|p| p.name == project).ok_or(StatusCode::NOT_FOUND)?;
    let slug = crate::transcript::project_slug(&proj.path);
    let path = format!("{}/.claude/projects/{}/{}.jsonl", std::env::var("HOME").unwrap_or_default(), slug, session_id);
    let (msgs, next, active) = crate::transcript::read_transcript(&path, q.from);
    Ok(Json(serde_json::json!({ "messages": msgs, "next": next, "active": active })))
}
```
`ChatBody`에 `#[serde(default)] pub resume_session_id: Option<String>` 추가, `chat_handler`에서 `let resume = b.resume_session_id.clone().or_else(|| st.sessions.get(&project));` 로 변경. 라우터에 `.route("/sessions/{project}", get(sessions_handler)).route("/transcript/{project}/{session_id}", get(transcript_handler))` (인증군).

- [ ] **Step 6: 통과 확인** — `cargo test --workspace` → green. `cargo build -p awb-server` → 성공.

- [ ] **Step 7: 커밋** — `... -m "feat(server): 세션 목록/트랜스크립트 엔드포인트 + /chat 특정 세션 resume (연속성)"` (+트레일러)

---

### Task 2: 데몬 /info(hostname) + 폰 멀티-PC store + api/타입 + vitest

**Files:** Modify `crates/awb-server/src/routes.rs`(+`/info`); Create `mobile/src/store/pcs.ts`, Modify `mobile/src/lib/{api,types}.ts`, `mobile/src/lib/api.test.ts`, Create `mobile/src/lib/pcs-util.ts` + `mobile/src/lib/pcs-util.test.ts`.

**Interfaces:**
- 데몬 `GET /info` (require_token 뒤) → `{ hostname: String }`. hostname = `scutil --get ComputerName` 성공 시 그 값, 실패 시 `hostname` 명령, 최종 폴백 `"Mac"`.
- 폰 `types.ts`: `PC { id: string; label: string; baseUrl: string; token: string }`, `SessionInfo { session_id, updated, preview, count, active }`, `TranscriptMsg { role, text, tools }`.
- 폰 `pcs-util.ts`(순수, 테스트 대상): `upsertPC(list, pc)` — 같은 baseUrl이면 교체(중복 방지), 아니면 추가; `pcId(baseUrl)` — baseUrl 기반 안정적 id.
- 폰 `pcs.ts`(SecureStore 래퍼): `loadPCs(): Promise<PC[]>`, `savePCs(list)`, `addPC(pc)`(upsert), `removePC(id)`, `getPC(id): Promise<PC|null>`. JSON 배열 저장(키 `awb_pcs`).
- 폰 `api.ts`: `info()`, `sessions(project)`, `transcript(project, sessionId, from)`, `chat(project, prompt, plan, resumeSessionId?)` (resume 시 body에 `resume_session_id`).

- [ ] **Step 1: 데몬 /info** — routes.rs에 `info_handler`(hostname) + 인증군 라우트 `.route("/info", get(info_handler))`. `cargo test --workspace` green 유지, `cargo build -p awb-server` 성공.
- [ ] **Step 2: 폰 실패 테스트** — `pcs-util.test.ts`(upsert 중복 baseUrl 교체·신규 추가, pcId 안정성) + `api.test.ts`에 sessions/transcript/chat-resume URL·body 검증 추가. `cd mobile && npx vitest run` → FAIL.
- [ ] **Step 3: 폰 구현** — types/pcs-util/pcs/api 작성. `pcs-util`은 RN/Expo import 없이 순수. `pcs.ts`만 expo-secure-store 사용.
- [ ] **Step 4: 통과 확인** — `npx vitest run` PASS, `npx tsc --noEmit` 0.
- [ ] **Step 5: 커밋** — `... -m "feat: 데몬 /info(hostname) + 폰 멀티-PC store + api sessions/transcript/resume"` (+트레일러)

---

### Task 3: 폰 — PC 목록 화면(진입) + 페어링이 PC 추가 + 프로젝트 화면 PC 스코프화

**Files:** Modify `mobile/app/index.tsx`(→ PC 목록), Create `mobile/app/projects.tsx`(기존 프로젝트 목록 이전, pc 스코프), Modify `mobile/app/pair.tsx`(PC 추가), `mobile/app/_layout.tsx`(타이틀·초기 라우팅).

**Interfaces:** 네비 계층: `index`(PC 목록) → `projects?pc=<id>` → `sessions/[project]?pc=<id>` → `chat/[project]?pc=<id>&session=<id>`. 각 화면은 `getPC(pc)`로 baseUrl/token 획득(단일 loadSession 대체).

- [ ] **Step 1: index = PC 목록** — 마운트 시 `loadPCs()`. 비면 `/pair`로. 목록: 각 PC(label·baseUrl 호스트) 탭 → `router.push({ pathname: "/projects", params: { pc: pc.id } })`. 상단 **[+ PC 추가]** → `/pair`. 각 PC 길게눌러 삭제(removePC).
- [ ] **Step 2: projects.tsx** — 기존 index의 프로젝트 목록 로직 이전. `useLocalSearchParams<{pc:string}>()` → `getPC(pc)` → `makeClient(baseUrl,token).projects()/preflight()`. 프로젝트 탭 → `sessions/[project]?pc=<id>&path=<path>`. 401 시 그 PC removePC 후 index로.
- [ ] **Step 3: pair.tsx PC 추가** — 페어링 성공 후 `client.info()`로 hostname 취득 → `addPC({ id: pcId(baseUrl), label: hostname, baseUrl, token })` → `router.replace("/")`(PC 목록). (loadSession/saveSession 단일 저장 제거.)
- [ ] **Step 4: _layout** — 초기 라우팅을 loadPCs 기준으로(없으면 pair). 화면 타이틀: index "PC", projects는 화면에서 PC label로 설정. push 알림 등록은 첫 PC 기준(있으면).
- [ ] **Step 5: 검증** — `npx tsc --noEmit` 0, `npx vitest run` 유지.
- [ ] **Step 6: 커밋** — `... -m "feat(mobile): PC 목록 진입 + 페어링 PC 추가 + 프로젝트 화면 PC 스코프"` (+트레일러)

---

### Task 4: 폰 — 세션 목록 화면(PC+프로젝트 스코프)

**Files:** Create `mobile/app/sessions/[project].tsx`; Modify `_layout.tsx`(타이틀).

**Interfaces:** `useLocalSearchParams<{project, pc, path}>()` → `getPC(pc)` → `makeClient(...).sessions(project)`. 상단 **[+ 새 대화]**(→ `chat/[project]?pc&path`, session 없음), 세션 리스트(preview·updated·🟢active) 탭 → `chat/[project]?pc&path&session=<id>`.

- [ ] **Step 1: sessions/[project].tsx** — loadPCs/getPC 가드 → sessions 로드 → FlatList(빈목록·로딩·에러·재시도). 토큰 미로깅.
- [ ] **Step 2: _layout 타이틀** — `sessions/[project]` 등록(프로젝트명은 화면에서).
- [ ] **Step 3: 검증** — `npx tsc --noEmit` 0.
- [ ] **Step 4: 커밋** — `... -m "feat(mobile): 세션 목록 화면(PC+프로젝트 스코프)"` (+트레일러)

---

### Task 5: 폰 — 채팅 세션 이어받기 + 트랜스크립트 로드 + 활성 실시간 + PC resume 안내

**Files:** Modify `mobile/app/chat/[project].tsx`.

**Interfaces:** params `{project, pc, path, session?}`. `getPC(pc)`로 client 구성(기존 loadSession 대체). session 있으면 마운트 시 `transcript(project, session, 0)` 로드→chat 메시지 초기화(TranscriptMsg→ChatMsg); active면 ~2s 폴로 `from=next` 증분 append. 전송 시 `chat(project, text, plan, session)`. **[PC에서 이어받기]** → `expo-clipboard`로 `cd <path> && claude --resume <session>` 복사. session 없으면 새 대화(기존 동작).

- [ ] **Step 1: PC 스코프화** — 기존 `loadSession()`을 `getPC(pc)`로 교체(baseUrl/token). 없으면 index로.
- [ ] **Step 2: 트랜스크립트 로드+활성 폴** — session 있으면 마운트 로드 + active 폴(정리 포함). nextLine ref.
- [ ] **Step 3: 전송 resume** — handleSend가 session 전달.
- [ ] **Step 4: PC 이어받기 안내** — expo-clipboard(설치: `npx expo install expo-clipboard`) 버튼(session 있을 때). ※ 순수 JS 모듈이면 재빌드 불필요, 네이티브면 재빌드(USB) 필요 — 구현자 확인 후 필요시 보고.
- [ ] **Step 5: 검증** — `npx tsc --noEmit` 0, `npx vitest run` 유지. 실기기 스모크(라이브 리로드): PC 세션 폰서 열기→과거 대화→이어하기→PC resume 명령.
- [ ] **Step 6: 커밋** — `... -m "feat(mobile): 채팅 세션 이어받기+활성 실시간+PC resume 안내"` (+트레일러)

---

## Plan 4 Self-Review (멀티-PC 반영)
- **Spec coverage:** 세션 목록/트랜스크립트/resume=Task1(데몬)+Task2(api)+Task4/5(폰); **멀티-PC 그룹화**=데몬 /info(Task2)+폰 멀티-PC store(Task2)+PC 목록 진입/페어링 추가(Task3); 활성 실시간·PC resume 안내=Task5. 무봉제 핸드오프 모델 유지.
- **Type consistency:** `PC{id,label,baseUrl,token}` Task2↔3↔4↔5; `SessionInfo`/`TranscriptMsg` 데몬↔폰; 모든 화면이 `pc` 파라미터→getPC로 client 구성(단일 loadSession 제거) 일관.
- **위험:** (1) 네비 재구성(단일→PC계층)이 Plan3 화면(index/chat)을 개편 — 기존 기능 회귀 없게 projects/chat 로직 보존 이전. (2) expo-clipboard 네이티브 여부에 따라 재빌드 필요 가능(Task5서 확인). (3) 슬러그 규약 실기기 대조(Task1 위험 유지).

## 후속(v2): 활성 세션 동시 진행 경고/락, 대용량 트랜스크립트 페이지네이션, PC label 편집, 세션 검색/이름, thinking 블록 접기.
