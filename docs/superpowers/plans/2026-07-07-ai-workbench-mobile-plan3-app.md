# ai-workbench Mobile — Plan 3: RN+Expo 모바일 앱

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`) syntax.

**Goal:** 폰에서 Mac 데몬(awb-server)에 Tailscale로 접속해 프롬프트를 실행·확인하는 React Native + Expo 앱. QR 페어링→토큰 저장, 프로젝트 목록, 멀티턴 채팅(WS 실시간 스트리밍·plan 토글·취소·완료 배지·git 요약), 완료 푸시 등록.

**Architecture:** `mobile/` 신규 Expo(TypeScript) 앱. 순수 로직(타입·API 클라이언트·페어링 URL 파싱·WS 이벤트 리듀서)은 UI와 분리해 vitest로 검증(헤드리스 가능). 화면은 얇게 — 순수 로직 + Expo API(카메라/SecureStore/notifications) 호출만. 데몬 base URL은 페어링 시 QR payload(`awb://<ip>:<port>?code=<code>`)에서 추출해 저장. 인증은 저장된 Bearer 토큰(HTTP 헤더 / WS는 `?token=`).

**Tech Stack:** Expo SDK 57, React Native, TypeScript, expo-router(파일 기반 네비), expo-camera(QR 스캔), expo-secure-store(토큰), expo-notifications(FCM 푸시 토큰). 테스트: vitest(순수 로직만; UI는 `tsc --noEmit` typecheck + 리뷰). 상태: React hooks(외부 상태관리 라이브러리 없음 — YAGNI).

## Global Constraints

- 커밋 트레일러(마지막 줄): `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`
- **앱은 데몬 API만 소비** — 서버 스키마(Plan 2a/2b)와 일치: `/pair?code=`→`{token,device_id}`, `/projects`→`[{name,path,last_activity,badge}]`, `POST /chat/{project}{prompt,plan}`→`{run_id,log}`, WS `/stream/{run_id}?offset=&token=`→이벤트 `{kind: token|tool_use|done|error, ...}`, `POST /cancel/{run_id}`, `GET /status/{run_id}`, `GET /diff?path=`, `GET /preflight`, `POST /push/register {token}`.
- WS 이벤트 종류(서버 streamevt.rs와 일치): `token{text}`, `tool_use{name,summary}`, `done{exit,verdict,changed_files}`, `error{message}`. verdict 문자열: `success`/`success(무변경)`/`failed`/`running`.
- 토큰·코드는 로그 출력 금지. 토큰은 SecureStore에만.
- 대상 **Android 우선**(EAS build→APK). iOS는 v2.
- 순수 로직은 `mobile/src/lib/`(UI import 없음)에 두고 vitest로 테스트. 화면은 `mobile/app/`.
- ⚠️ 실기기 구동·EAS 빌드·FCM 크리덴셜은 사용자 몫 — 이 플랜은 코드 + typecheck + 순수로직 테스트까지.

## File Structure (Plan 3 범위)

```
mobile/
  package.json, app.json, tsconfig.json, vitest.config.ts   스캐폴드
  src/lib/
    types.ts        서버 DTO 타입(Project/Preflight/WS Event/ChatState)
    api.ts          데몬 HTTP 클라이언트(base URL+토큰 주입, 엔드포인트 함수)
    pairing.ts      QR payload 파싱(awb://ip:port?code=) → {baseUrl, code}
    events.ts       WS 이벤트 리듀서(reduceEvent) + verdictLabel + 초기상태
    api.test.ts, pairing.test.ts, events.test.ts   vitest
  src/store/
    session.ts      SecureStore 래퍼(base URL+토큰 저장/로드/삭제)
  app/
    _layout.tsx     expo-router 루트(페어링 여부로 분기)
    pair.tsx        QR 스캔 → /pair → 토큰 저장
    index.tsx       프로젝트 목록
    chat/[project].tsx   채팅(멀티턴·WS·plan·취소·완료배지·git요약)
```

---

### Task 1: Expo 앱 스캐폴드 + 설정 + vitest

**Files:** Create `mobile/` (Expo TS 앱), `mobile/vitest.config.ts`, `mobile/tsconfig.json` 조정.

**Interfaces:** Produces: 빌드·타입체크되는 빈 Expo 앱 + vitest 러너(순수 로직 테스트용). expo-router/expo-camera/expo-secure-store/expo-notifications 의존 설치.

- [ ] **Step 1: Expo 앱 생성**
```bash
cd /Users/mh/github/ai-workbench/.claude/worktrees/mobile-plan1-core-lib
npx create-expo-app@latest mobile --template blank-typescript --no-install
cd mobile && npm install
npx expo install expo-router expo-camera expo-secure-store expo-notifications react-native-safe-area-context react-native-screens expo-linking expo-constants
npm install -D vitest
```
(create-expo-app이 네트워크/대화형으로 막히면: 수동으로 `mobile/package.json`·`app.json`·`tsconfig.json`·`App.tsx` 최소 스캐폴드 작성 후 `npm install` — 구현자 재량. 목표는 `npx tsc --noEmit` 통과하는 Expo TS 프로젝트.)

- [ ] **Step 2: expo-router 진입 설정** — `mobile/package.json`의 `main`을 `expo-router/entry`로, `app.json`에 `"scheme": "awb"`, plugins에 `expo-router`·`expo-camera`(카메라 권한 문구). `app/` 디렉토리 생성.

- [ ] **Step 3: vitest 설정** — Create `mobile/vitest.config.ts`:
```ts
import { defineConfig } from "vitest/config";
export default defineConfig({ test: { environment: "node", include: ["src/**/*.test.ts"] } });
```
`mobile/package.json` scripts에 `"test": "vitest run"`, `"typecheck": "tsc --noEmit"` 추가.

- [ ] **Step 4: 스모크 파일 + 검증** — Create `mobile/src/lib/types.ts`(빈 export 하나) 후:
Run: `cd mobile && npx tsc --noEmit` → 에러 0. `npx vitest run` → 0 tests(러너 동작 확인).

- [ ] **Step 5: 커밋** — `git add -A && git commit -m "$(printf 'feat(mobile): Expo TS 앱 스캐폴드 + expo-router/camera/secure-store/notifications + vitest\n\nCo-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>')"`

---

### Task 2: 순수 로직 — 타입·API 클라이언트·페어링 파싱·WS 이벤트 리듀서 (vitest)

**Files:** Create `mobile/src/lib/{types,api,pairing,events}.ts` + `{api,pairing,events}.test.ts`.

**Interfaces:**
- Produces: `types.ts` — `Project`, `Preflight`, `Badge`, `WsEvent`(token/tool_use/done/error 판별합집합), `ChatState`, `ChatMsg`.
- Produces: `pairing.ts` — `parsePairPayload(s: string) -> {baseUrl, code} | null` (`awb://<ip>:<port>?code=<code>` → `{baseUrl:"http://<ip>:<port>", code}`).
- Produces: `api.ts` — `makeClient(baseUrl, token)` 반환 객체: `projects()`, `preflight()`, `chat(project, prompt, plan)`, `status(runId)`, `cancel(runId)`, `diff(path)`, `registerPush(token)`; + `pairUrl(baseUrl, code)`, `streamUrl(baseUrl, runId, offset, token)`(WS URL 빌더).
- Produces: `events.ts` — `initialChatState()`, `reduceEvent(state, ev) -> ChatState`(token 누적→현재 assistant 말풍선, tool_use→칩, done→verdict/종료, error→에러표시), `verdictLabel(verdict) -> string`.

- [ ] **Step 1: 실패 테스트 — pairing 파싱**
```ts
// pairing.test.ts
import { describe, it, expect } from "vitest";
import { parsePairPayload } from "./pairing";
describe("parsePairPayload", () => {
  it("valid awb URL", () => {
    expect(parsePairPayload("awb://100.64.0.1:8787?code=ABC234"))
      .toEqual({ baseUrl: "http://100.64.0.1:8787", code: "ABC234" });
  });
  it("rejects non-awb", () => { expect(parsePairPayload("https://x?code=1")).toBeNull(); });
  it("rejects missing code", () => { expect(parsePairPayload("awb://1.2.3.4:80")).toBeNull(); });
});
```
```ts
// events.test.ts
import { describe, it, expect } from "vitest";
import { initialChatState, reduceEvent, verdictLabel } from "./events";
describe("reduceEvent", () => {
  it("accumulates tokens into current assistant message", () => {
    let s = initialChatState();
    s = reduceEvent(s, { kind: "token", text: "안" });
    s = reduceEvent(s, { kind: "token", text: "녕" });
    expect(s.messages.at(-1)).toMatchObject({ role: "assistant", text: "안녕" });
    expect(s.running).toBe(true);
  });
  it("done sets verdict and stops running", () => {
    let s = reduceEvent(initialChatState(), { kind: "done", exit: 0, verdict: "success", changed_files: 2 });
    expect(s.running).toBe(false); expect(s.verdict).toBe("success");
  });
});
describe("verdictLabel", () => {
  it("labels", () => {
    expect(verdictLabel("success")).toMatch(/완료/);
    expect(verdictLabel("failed")).toMatch(/실패/);
    expect(verdictLabel("success(무변경)")).toMatch(/변경 없음/);
  });
});
```
```ts
// api.test.ts
import { describe, it, expect } from "vitest";
import { pairUrl, streamUrl, makeClient } from "./api";
describe("url builders", () => {
  it("streamUrl uses ws scheme + token + offset", () => {
    expect(streamUrl("http://1.2.3.4:8787", "r1", 0, "tok"))
      .toBe("ws://1.2.3.4:8787/stream/r1?offset=0&token=tok");
  });
  it("pairUrl", () => { expect(pairUrl("http://1.2.3.4:8787","AB")).toBe("http://1.2.3.4:8787/pair?code=AB"); });
  it("client.chat posts to /chat/:project", async () => {
    const calls: any[] = [];
    const fetchMock = async (url: string, init: any) => { calls.push([url, init]); return { ok: true, json: async () => ({ run_id: "r1", log: "l" }) }; };
    const c = makeClient("http://1.2.3.4:8787", "tok", fetchMock as any);
    const r = await c.chat("demo", "hi", true);
    expect(r.run_id).toBe("r1");
    expect(calls[0][0]).toBe("http://1.2.3.4:8787/chat/demo");
    expect(JSON.parse(calls[0][1].body)).toEqual({ prompt: "hi", plan: true });
    expect(calls[0][1].headers.Authorization).toBe("Bearer tok");
  });
});
```

- [ ] **Step 2: 실패 확인** — Run: `cd mobile && npx vitest run` → FAIL(모듈 없음).

- [ ] **Step 3: 구현** — `types.ts`:
```ts
export interface Badge { todo: number; doing: number; done: number; updated: string }
export interface Project { name: string; path: string; last_activity: number; badge: Badge | null }
export interface Check { id: string; ok: boolean; detail: string }
export interface Preflight { claude_path: string | null; checks: Check[] }
export type WsEvent =
  | { kind: "token"; text: string }
  | { kind: "tool_use"; name: string; summary: string }
  | { kind: "done"; exit: number | null; verdict: string; changed_files: number }
  | { kind: "error"; message: string };
export interface ChatMsg { role: "user" | "assistant"; text: string; tools?: string[] }
export interface ChatState { messages: ChatMsg[]; running: boolean; verdict: string | null; changedFiles: number; error: string | null }
```
`pairing.ts`:
```ts
export function parsePairPayload(s: string): { baseUrl: string; code: string } | null {
  if (!s.startsWith("awb://")) return null;
  const rest = s.slice("awb://".length);
  const [hostPort, query] = rest.split("?");
  if (!hostPort || !query) return null;
  const params = new URLSearchParams(query);
  const code = params.get("code");
  if (!code) return null;
  return { baseUrl: `http://${hostPort}`, code };
}
```
`api.ts`:
```ts
import type { Project, Preflight } from "./types";
export function pairUrl(baseUrl: string, code: string) { return `${baseUrl}/pair?code=${encodeURIComponent(code)}`; }
export function streamUrl(baseUrl: string, runId: string, offset: number, token: string) {
  const ws = baseUrl.replace(/^http/, "ws");
  return `${ws}/stream/${runId}?offset=${offset}&token=${encodeURIComponent(token)}`;
}
type F = typeof fetch;
export function makeClient(baseUrl: string, token: string, f: F = fetch) {
  const h = { Authorization: `Bearer ${token}` };
  const jget = async (p: string) => { const r = await f(`${baseUrl}${p}`, { headers: h } as any); if (!(r as any).ok) throw new Error(`${p} ${(r as any).status}`); return (r as any).json(); };
  return {
    projects: (): Promise<Project[]> => jget("/projects"),
    preflight: (): Promise<Preflight> => jget("/preflight"),
    diff: (path: string) => jget(`/diff?path=${encodeURIComponent(path)}`),
    status: (runId: string) => jget(`/status/${runId}`),
    chat: async (project: string, prompt: string, plan: boolean) => {
      const r = await f(`${baseUrl}/chat/${encodeURIComponent(project)}`, { method: "POST", headers: { ...h, "Content-Type": "application/json" }, body: JSON.stringify({ prompt, plan }) } as any);
      if (!(r as any).ok) throw new Error(`chat ${(r as any).status}`);
      return (r as any).json() as Promise<{ run_id: string; log: string }>;
    },
    cancel: (runId: string) => f(`${baseUrl}/cancel/${runId}`, { method: "POST", headers: h } as any),
    registerPush: (pushToken: string) => f(`${baseUrl}/push/register`, { method: "POST", headers: { ...h, "Content-Type": "application/json" }, body: JSON.stringify({ token: pushToken }) } as any),
  };
}
```
`events.ts`:
```ts
import type { ChatState, WsEvent } from "./types";
export function initialChatState(): ChatState { return { messages: [], running: true, verdict: null, changedFiles: 0, error: null }; }
export function verdictLabel(v: string): string {
  if (v.startsWith("success")) return v.includes("무변경") ? "✅ 완료(변경 없음)" : "✅ 완료";
  if (v === "failed") return "❌ 실패";
  return "⏳ 실행 중";
}
export function reduceEvent(state: ChatState, ev: WsEvent): ChatState {
  const s: ChatState = { ...state, messages: [...state.messages] };
  const last = s.messages.at(-1);
  const ensureAssistant = () => {
    if (!last || last.role !== "assistant") { const m = { role: "assistant" as const, text: "", tools: [] as string[] }; s.messages.push(m); return m; }
    const m = { ...last, tools: [...(last.tools ?? [])] }; s.messages[s.messages.length - 1] = m; return m;
  };
  switch (ev.kind) {
    case "token": { const m = ensureAssistant(); m.text += ev.text; break; }
    case "tool_use": { const m = ensureAssistant(); m.tools!.push(ev.name); break; }
    case "done": { s.running = false; s.verdict = ev.verdict; s.changedFiles = ev.changed_files; break; }
    case "error": { s.error = ev.message; s.running = false; break; }
  }
  return s;
}
```

- [ ] **Step 4: 통과 확인** — Run: `cd mobile && npx vitest run` → PASS(전체). `npx tsc --noEmit` → 에러 0.

- [ ] **Step 5: 커밋** — `... -m "feat(mobile): 순수 로직(types/api/pairing/events) + vitest"` (+트레일러)

---

### Task 3: 세션 저장 + 페어링 화면 (QR 스캔)

**Files:** Create `mobile/src/store/session.ts`, `mobile/app/pair.tsx`, `mobile/app/_layout.tsx`.

**Interfaces:**
- Produces: `session.ts` — `saveSession(baseUrl, token)`, `loadSession() -> {baseUrl, token} | null`, `clearSession()` (expo-secure-store).
- `pair.tsx` — expo-camera로 QR 스캔 → `parsePairPayload` → `GET pairUrl` → 응답 토큰 + baseUrl `saveSession` → 목록으로 이동.
- `_layout.tsx` — 앱 시작 시 `loadSession()` 있으면 index, 없으면 pair로.

- [ ] **Step 1: session.ts 순수-ish 테스트** — SecureStore는 네이티브라 vitest 불가; `session.ts`는 SecureStore 호출만 감싸는 얇은 래퍼로 두고 **로직 없음**(테스트 생략, 리뷰로 확인). 대신 이 태스크의 검증은 `npx tsc --noEmit`.

- [ ] **Step 2: 구현** — `session.ts`:
```ts
import * as SecureStore from "expo-secure-store";
const K_BASE = "awb_base_url", K_TOK = "awb_token";
export async function saveSession(baseUrl: string, token: string) { await SecureStore.setItemAsync(K_BASE, baseUrl); await SecureStore.setItemAsync(K_TOK, token); }
export async function loadSession() { const baseUrl = await SecureStore.getItemAsync(K_BASE); const token = await SecureStore.getItemAsync(K_TOK); return baseUrl && token ? { baseUrl, token } : null; }
export async function clearSession() { await SecureStore.deleteItemAsync(K_BASE); await SecureStore.deleteItemAsync(K_TOK); }
```
`pair.tsx` (핵심 흐름): expo-camera `CameraView` `onBarcodeScanned` → `parsePairPayload(data)` → 유효하면 `fetch(pairUrl(baseUrl, code))` → `{token}` → `saveSession(baseUrl, token)` → `router.replace("/")`. 실패 시 에러 문구. 카메라 권한 요청 처리. **토큰/코드 콘솔 로그 금지.**
`_layout.tsx`: expo-router `Stack`; 마운트 시 `loadSession()` 결과로 초기 라우트 결정(없으면 `/pair`로 `redirect`).

- [ ] **Step 3: 검증** — Run: `cd mobile && npx tsc --noEmit` → 에러 0. (카메라/스캔 실동작은 사용자 실기기 검증.)

- [ ] **Step 4: 커밋** — `... -m "feat(mobile): 세션 SecureStore 저장 + QR 페어링 화면"` (+트레일러)

---

### Task 4: 프로젝트 목록 화면

**Files:** Create `mobile/app/index.tsx`.

**Interfaces:** Consumes: `loadSession`, `makeClient(...).projects()/preflight()`, `types`. Produces: 목록 화면 — 프로젝트 이름·경로·worklog 배지(⬜/🔄/✅) 표시, 상단 프리플라이트 상태(claude 경로 OK 여부), 탭 → `chat/[project]`.

- [ ] **Step 1: 구현** — `index.tsx`: 마운트 시 `loadSession()`→없으면 `/pair` redirect. `makeClient(base,token).projects()` 로드해 `FlatList`. 각 항목 탭 → `router.push(\`/chat/${name}\`)`. `preflight()`로 claude 준비 배너. 당김 새로고침(선택). 에러 시 재시도 버튼. 토큰 로그 금지.

- [ ] **Step 2: 검증** — Run: `cd mobile && npx tsc --noEmit` → 0. `npx vitest run`(기존 순수로직 여전히 green).

- [ ] **Step 3: 커밋** — `... -m "feat(mobile): 프로젝트 목록 화면(+프리플라이트·배지)"` (+트레일러)

---

### Task 5: 채팅 화면 (WS 스트리밍·plan·취소·완료배지·git요약) + 푸시 등록

**Files:** Create `mobile/app/chat/[project].tsx`; Modify `mobile/app/_layout.tsx`(푸시 등록 초기화).

**Interfaces:** Consumes: `makeClient`, `streamUrl`, `reduceEvent`/`initialChatState`/`verdictLabel`, `session`, expo-notifications. Produces: 멀티턴 채팅 화면.

- [ ] **Step 1: 채팅 화면 구현** — `chat/[project].tsx`:
  - 입력 textarea + `[plan]` 토글 + `[전송]`/`[취소]`.
  - 전송: `client.chat(project, prompt, plan)` → `{run_id}` → `new WebSocket(streamUrl(base, run_id, 0, token))`. 사용자 말풍선 추가, `state=initialChatState()`.
  - WS `onmessage`: `JSON.parse` → `reduceEvent(state, ev)`로 상태 갱신(토큰 실시간 append, tool_use 칩). `done`이면 `verdictLabel` 배지 + `client.diff(projectPath)`로 변경요약 펼침 + WS close.
  - 취소: `client.cancel(run_id)`.
  - 재접속: WS `onclose`가 완료 전이면 마지막 offset으로 재연결(간단 재시도 1회) — offset은 수신 바이트 누적이 아니라 서버가 라인 기반이므로 v1은 `offset=0` 재생 허용(간단). (정교한 offset 추적은 후속.)
  - 토큰/코드 콘솔 로그 금지.
- [ ] **Step 2: 푸시 등록** — `_layout.tsx`(또는 index 마운트): expo-notifications 권한 요청 → `getExpoPushTokenAsync()` → `makeClient(base,token).registerPush(expoToken)`. 실패는 무시(푸시는 선택적 부가기능).

- [ ] **Step 3: 검증** — Run: `cd mobile && npx tsc --noEmit` → 0. `npx vitest run` → 순수로직 green. (WS·푸시 실동작은 사용자 실기기+데몬 스모크.)

- [ ] **Step 4: 커밋** — `... -m "feat(mobile): 채팅 화면(WS 스트림·plan·취소·완료배지·git요약) + 푸시 등록"` (+트레일러)

---

## Plan 3 Self-Review

- **Spec coverage:** 페어링(QR)=Task3; 프로젝트목록=Task4; 멀티턴 채팅·WS 스트리밍·plan토글·취소·완료배지·git요약=Task5; 푸시 등록=Task5; 순수로직(파싱/이벤트/API)=Task2(테스트됨); 스캐폴드=Task1. 서버 API·WS 이벤트 스키마는 Plan 2a/2b와 일치(제약에 명시).
- **Placeholder scan:** 순수 로직·스캐폴드는 완전 코드+테스트. UI 화면은 동작·데이터흐름·API/이벤트 배선을 정확히 명시하고 RN 보일러플레이트는 구현자 작성(UI는 TDD 불가라 typecheck+리뷰 게이트 — 의도적, 제약에 명시). WS offset 재접속은 v1 단순화(offset=0 재생) 명시.
- **Type consistency:** WsEvent 종류(token/tool_use/done/error) + verdict 문자열이 서버 streamevt.rs·runlog verdict와 일치. `makeClient` 엔드포인트 경로가 서버 라우트와 일치(/chat/:project, /stream/:run_id, /cancel/:run_id, /status/:run_id, /diff?path=, /projects, /preflight, /push/register, /pair?code=).
- **위험:** (1) create-expo-app 대화형/네트워크 → 실패 시 수동 스캐폴드 폴백 명시. (2) UI 런타임 미검증 → typecheck+순수로직테스트+리뷰로 최대한 보증, 실구동은 사용자 기기. (3) WS offset 재접속 단순화 — 정교화는 후속. (4) expo-notifications 푸시는 FCM 크리덴셜(사용자) 없으면 토큰획득 실패 → 선택적으로 처리(앱 크래시 안 함).

## v1 마감
- Plan 1(awb-core)+2a+2b(awb-server)+3(mobile) = v1 코드 완성. **사용자 최종 검증(필수):** 데몬 실행→폰 EAS APK 설치→Tailscale→QR 페어링→프로젝트 선택→프롬프트 실행/스트림/취소→완료 푸시. FCM 크리덴셜·EAS 빌드·기기 설정은 사용자.
- 후속(v2): iOS/APNs, WoL, 턴 큐잉, 대화 히스토리 영구저장, diff 뷰어, 정교한 WS offset 재접속, chat 에러 5xx 세분화, hung-run 워처 cap, runlog UTF-8 청크 하드닝.
