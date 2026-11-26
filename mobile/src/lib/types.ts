export interface Badge { todo: number; doing: number; done: number; updated: string }
export interface Project { name: string; path: string; last_activity: number; badge: Badge | null; agent_status?: "working" | "waiting" | null;
  /** 앱에 보이는 세션(/sessions 와 같은 기준, 보관 제외). 옛 데몬은 보내지 않는다 */
  sessions?: SessionInfo[] }
export interface Check { id: string; ok: boolean; detail: string }
export interface Preflight { claude_path: string | null; checks: Check[] }
export type WsEvent =
  | { kind: "token"; text: string }
  | { kind: "tool_use"; name: string; summary: string }
  | { kind: "done"; exit: number | null; verdict: string; changed_files: number; session_id?: string | null }
  | { kind: "error"; message: string };
export interface ChatMsg { role: "user" | "assistant"; text: string; tools?: string[]; toolDetails?: string[]; options?: string[]; todos?: string[];
  /** 채널 전송 직후 먼저 붙인 말풍선 — 대화 기록에 같은 메시지가 오면 뺀다(chatMsgs#appendPolled) */
  pending?: boolean }
export interface ChatState { messages: ChatMsg[]; running: boolean; verdict: string | null; changedFiles: number; error: string | null }

/** 페어링된 PC 하나(멀티-PC 지원). id는 baseUrl에서 파생된 안정적 식별자(pcs-util#pcId 참고). */
export interface PC { id: string; label: string; baseUrl: string; token: string; lastSeen?: number;
  /** 페어링 응답의 device_id — 기기 목록에서 이 폰을 가리킨다. 이 필드 이전에 페어링한 PC 엔 없다 */
  deviceId?: string }
/** 데몬 `/sessions/:project` 응답 항목. */
export interface SessionInfo { session_id: string; updated: number; preview: string; answer_preview: string; count: number; active: boolean; waiting: boolean; live?: boolean; app?: boolean; name?: string | null; archived?: boolean;
  /** 격리 트리에서 돈 세션이면 그 트리. 옛 데몬은 보내지 않는다 */
  worktree?: Worktree;
  /** 터미널 세션에 폰 채널(plugins/phone)이 붙어 있음 — 보내면 갈라지지 않고 그 터미널 대화에 들어간다 */
  channel?: boolean }
/** 폰 격리 실행의 작업 트리(`<저장소>.wt-phone-<id>`, 브랜치 claude/phone-<id>). removed 면 정리돼 이어갈 수 없다 */
export interface Worktree { path: string; branch: string; removed?: boolean }
/** 세션 끝내기·보관 때 격리 트리 정리 결과 — removed=false 면 reason 이 남긴 이유 */
export interface WorktreeCleanup { path: string; branch: string; removed: boolean; branch_deleted?: boolean; reason?: string }
/** 데몬 `/transcript/:project/:sessionId` 응답의 메시지 항목. */
export interface TranscriptMsg { role: string; text: string; tools: string[]; tool_details: string[]; options?: string[]; todos?: string[] }
/** /info.rate_limit — claude 스트림의 마지막 rate_limit_event. status: allowed | allowed_warning | rejected(차단) */
export interface RateLimit { status: string; resets_at: number | null; rate_limit_type: string | null; session_id: string | null; project: string }
/** 한도 리셋 때 이어가기 예약(프로젝트당 1개) */
export interface ResumeReservation { session_id: string; resets_at: number; mode?: string; model?: string | null; effort?: string | null }
/** 요일·시각 반복 실행 예약(프로젝트당 여러 개). days: 0=일…6=토, time: Mac 로컬 "HH:MM", next_at: 다음 회차 epoch초 */
export interface Schedule { id: string; project: string; days: number[]; time: string; prompt: string; mode: string; model?: string | null; effort?: string | null; next_at: number }
/** 데몬 `/needs-you` — 이 PC 의 모든 프로젝트에서 권한 요청과 질문으로 끝난(답 대기) 세션 */
export interface NeedsYou {
  permissions: { id: string; project: string; tool_name: string; summary: string }[];
  waiting: (SessionInfo & { project: string; path: string })[];
}
