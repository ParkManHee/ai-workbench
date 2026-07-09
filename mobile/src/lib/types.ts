export interface Badge { todo: number; doing: number; done: number; updated: string }
export interface Project { name: string; path: string; last_activity: number; badge: Badge | null; agent_status?: "working" | "waiting" | null }
export interface Check { id: string; ok: boolean; detail: string }
export interface Preflight { claude_path: string | null; checks: Check[] }
export type WsEvent =
  | { kind: "token"; text: string }
  | { kind: "tool_use"; name: string; summary: string }
  | { kind: "done"; exit: number | null; verdict: string; changed_files: number }
  | { kind: "error"; message: string };
export interface ChatMsg { role: "user" | "assistant"; text: string; tools?: string[]; toolDetails?: string[]; options?: string[] }
export interface ChatState { messages: ChatMsg[]; running: boolean; verdict: string | null; changedFiles: number; error: string | null }

/** 페어링된 PC 하나(멀티-PC 지원). id는 baseUrl에서 파생된 안정적 식별자(pcs-util#pcId 참고). */
export interface PC { id: string; label: string; baseUrl: string; token: string }
/** 데몬 `/sessions/:project` 응답 항목. */
export interface SessionInfo { session_id: string; updated: number; preview: string; answer_preview: string; count: number; active: boolean; waiting: boolean }
/** 데몬 `/transcript/:project/:sessionId` 응답의 메시지 항목. */
export interface TranscriptMsg { role: string; text: string; tools: string[]; tool_details: string[]; options?: string[] }
