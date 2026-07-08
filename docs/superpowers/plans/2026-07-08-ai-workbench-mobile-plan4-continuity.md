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

### Task 2: 폰 — api/타입 확장 + vitest

**Files:** Modify `mobile/src/lib/{api,types}.ts`; `mobile/src/lib/api.test.ts`.

**Interfaces:**
- `types.ts`: `SessionInfo { session_id, updated, preview, count, active }`, `TranscriptMsg { role, text, tools }`.
- `api.ts`: `sessions(project): Promise<SessionInfo[]>` (`GET /sessions/:project`), `transcript(project, sessionId, from): Promise<{messages: TranscriptMsg[]; next: number; active: boolean}>` (`GET /transcript/:project/:sessionId?from=`), `chat(project, prompt, plan, resumeSessionId?)` — body에 `resume_session_id` 포함(있을 때).

- [ ] **Step 1: 실패 테스트 (api.test.ts)**
```ts
it("sessions/transcript/chat-resume URLs", async () => {
  const calls: any[] = [];
  const f = async (u: string, init?: any) => { calls.push([u, init]); return { ok: true, json: async () => ([]) }; };
  const c = makeClient("http://1.2.3.4:8787", "tok", f as any);
  await c.sessions("demo"); expect(calls[0][0]).toBe("http://1.2.3.4:8787/sessions/demo");
  await c.transcript("demo", "s1", 5); expect(calls[1][0]).toBe("http://1.2.3.4:8787/transcript/demo/s1?from=5");
  const f2calls: any[] = [];
  const f2 = async (u: string, init: any) => { f2calls.push([u, init]); return { ok: true, json: async () => ({ run_id: "r", log: "l" }) }; };
  const c2 = makeClient("http://1.2.3.4:8787", "tok", f2 as any);
  await c2.chat("demo", "hi", false, "sess-9");
  expect(JSON.parse(f2calls[0][1].body)).toEqual({ prompt: "hi", plan: false, resume_session_id: "sess-9" });
});
```

- [ ] **Step 2: 실패 확인** — `cd mobile && npx vitest run` → FAIL.

- [ ] **Step 3: 구현** — `api.ts`에 추가:
```ts
sessions: (project: string): Promise<SessionInfo[]> => jget(`/sessions/${encodeURIComponent(project)}`),
transcript: (project: string, sessionId: string, from = 0): Promise<{ messages: TranscriptMsg[]; next: number; active: boolean }> =>
  jget(`/transcript/${encodeURIComponent(project)}/${encodeURIComponent(sessionId)}?from=${from}`),
```
`chat`에 4번째 인자 `resumeSessionId?: string` 추가 → body에 `...(resumeSessionId ? { resume_session_id: resumeSessionId } : {})`. `types.ts`에 SessionInfo/TranscriptMsg 추가.

- [ ] **Step 4: 통과 확인** — `npx vitest run` → PASS. `npx tsc --noEmit` → 0.

- [ ] **Step 5: 커밋** — `... -m "feat(mobile): api sessions/transcript + chat resume 옵션"` (+트레일러)

---

### Task 3: 폰 — 세션 목록 화면 + 프로젝트 진입 흐름 변경

**Files:** Create `mobile/app/sessions/[project].tsx`; Modify `mobile/app/index.tsx`(프로젝트 탭 → sessions로), `mobile/app/_layout.tsx`(타이틀).

**Interfaces:** Consumes `makeClient().sessions()`, `loadSession`. Produces: 세션 목록 화면 — 상단 **[+ 새 대화]**(→ chat/[project] resume 없이), 아래 세션 리스트(preview·updated·🟢활성 배지), 탭 → `chat/[project]?session=<id>`.

- [ ] **Step 1: index 진입 변경** — `index.tsx`의 `handlePress`가 `sessions/[project]`로 이동하도록:
```tsx
router.push({ pathname: "/sessions/[project]", params: { project: project.name, path: project.path } });
```
- [ ] **Step 2: sessions/[project].tsx 구현** — 마운트 시 loadSession 가드 → `client.sessions(project)` 로드 → FlatList. 헤더에 "새 대화" 버튼(→ `chat/[project]` with params project,path, session 없음). 각 세션 탭 → `chat/[project]` with params {project, path, session: session_id}. active면 🟢. 에러/로딩/빈목록 처리. 토큰 미로깅.
- [ ] **Step 3: _layout에 세션 화면 타이틀** — `<Stack.Screen name="sessions/[project]" options={{ title: "세션" }} />` (chat처럼 화면에서 프로젝트명으로 덮어써도 됨).
- [ ] **Step 4: 검증** — `cd mobile && npx tsc --noEmit` → 0; `npx vitest run` → 유지.
- [ ] **Step 5: 커밋** — `... -m "feat(mobile): 세션 목록 화면 + 프로젝트→세션 진입"` (+트레일러)

---

### Task 4: 폰 — 채팅에 세션 이어받기 + 트랜스크립트 로드 + 활성 실시간 + PC resume 안내

**Files:** Modify `mobile/app/chat/[project].tsx`.

**Interfaces:** `useLocalSearchParams`에서 `session?: string` 수신. 있으면: 마운트 시 `client.transcript(project, session, 0)`로 과거 메시지를 chat state에 로드(TranscriptMsg→ChatMsg). 활성(active)이면 ~2s 폴로 `from=next`부터 증분 로드해 최신 반영. 전송 시 `client.chat(project, text, plan, session)`으로 그 세션 resume. 화면에 **[PC에서 이어받기]** 버튼 → `cd <path> && claude --resume <session>` 문자열을 표시(복사 가능, Clipboard). session 없으면 기존처럼 새 대화.

- [ ] **Step 1: 트랜스크립트 로드** — session 파라미터 있으면 마운트 useEffect에서 transcript 로드 → `setChat(s => ({ ...s, messages: msgs.map(m => ({ role: m.role, text: m.text, tools: m.tools })) }))`. `nextLine` ref 저장.
- [ ] **Step 2: 활성 실시간 폴** — active면 setInterval(2s)로 `transcript(project, session, nextLine)` → 새 메시지 append + nextLine 갱신. 화면 벗어나면/비활성 되면 정리.
- [ ] **Step 3: 전송 시 resume** — `handleSend`의 `client.chat(project, text, plan)` → `client.chat(project, text, plan, session)` (session 있으면 그 세션 이어감).
- [ ] **Step 4: PC 이어받기 안내** — 상단 또는 메뉴에 버튼 → `expo-clipboard`로 `cd <path> && claude --resume <session>` 복사 + 토스트/알림. (session 있을 때만 노출.)
- [ ] **Step 5: 검증** — `npx tsc --noEmit` → 0; `npx vitest run` 유지. 실기기 스모크(라이브 리로드): PC 세션 폰에서 열기→과거 대화 보임→이어하기→PC resume 명령 표시.
- [ ] **Step 6: 커밋** — `... -m "feat(mobile): 채팅 세션 이어받기(트랜스크립트 로드+활성 실시간+resume)+PC resume 안내"` (+트레일러)

---

## Plan 4 Self-Review
- **Spec coverage:** 과거 대화 보기=Task1(트랜스크립트)+Task4(로드/렌더); 세션 목록=Task1(/sessions)+Task3; 이어받기(resume)=Task1(/chat resume)+Task4; 활성 실시간 보기=Task1(active+from offset)+Task4(폴); PC 이어받기 안내=Task4. 무봉제 핸드오프 모델 반영(읽기+resume, 동시조종 배제).
- **Placeholder scan:** 데몬/파서/테스트 실제 코드. 폰 UI는 동작·API 배선 명시(UI는 typecheck+리뷰, 실동작은 라이브리로드 스모크).
- **Type consistency:** SessionInfo/TranscriptMsg 데몬↔폰 필드 일치; `from`(라인 offset) 데몬(read_transcript)↔api(transcript)↔chat 화면 일관; chat resume_session_id 데몬 ChatBody↔api chat↔화면 param.
- **위험:** (1) 슬러그 규약이 Claude 버전에 따라 다를 수 있음 → 실기기에서 실제 디렉토리와 대조(구현자 확인). (2) 동시 resume 충돌은 모델상 배제(핸드오프)지만, 활성 세션 resume 시 사용자에게 "PC에서 진행 중일 수 있음" 경고는 후속. (3) 큰 트랜스크립트 성능 — v1은 라인 기반 증분, 대용량 페이지네이션은 후속.

## 후속(v2): 활성 세션 동시 진행 경고/락 표시, 대용량 트랜스크립트 페이지네이션, 슬러그 규약 자동 검출, thinking 블록 접기, 세션 검색/이름.
