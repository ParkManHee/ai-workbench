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
