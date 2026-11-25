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
export interface ChatMsg { role: "user" | "assistant"; text: string; tools?: string[]; toolDetails?: string[]; options?: string[]; todos?: string[] }
export interface ChatState { messages: ChatMsg[]; running: boolean; verdict: string | null; changedFiles: number; error: string | null }

/** 페어링된 PC 하나(멀티-PC 지원). id는 baseUrl에서 파생된 안정적 식별자(pcs-util#pcId 참고). */
export interface PC { id: string; label: string; baseUrl: string; token: string; lastSeen?: number;
  /** 페어링 응답의 device_id — 기기 목록에서 이 폰을 가리킨다. 이 필드 이전에 페어링한 PC 엔 없다 */
  deviceId?: string }
/** 데몬 `/sessions/:project` 응답 항목. */
export interface SessionInfo { session_id: string; updated: number; preview: string; answer_preview: string; count: number; active: boolean; waiting: boolean; live?: boolean; app?: boolean; name?: string | null; archived?: boolean }
/** 데몬 `/transcript/:project/:sessionId` 응답의 메시지 항목. */
export interface TranscriptMsg { role: string; text: string; tools: string[]; tool_details: string[]; options?: string[]; todos?: string[] }
/** /info.rate_limit — claude 스트림의 마지막 rate_limit_event. status: allowed | allowed_warning | rejected(차단) */
export interface RateLimit { status: string; resets_at: number | null; rate_limit_type: string | null; session_id: string | null; project: string }
/** 한도 리셋 때 이어가기 예약(프로젝트당 1개) */
export interface ResumeReservation { session_id: string; resets_at: number; mode?: string; model?: string | null; effort?: string | null }
