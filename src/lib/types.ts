export interface Project { name: string; path: string; /* has_origin: boolean; */ last_activity: number }
export interface Check { id: string; ok: boolean; detail: string }
export interface Preflight { claude_path: string | null; checks: Check[] }
export interface Badge { todo: number; doing: number; done: number; updated: string }
export interface RunHandle { log: string; pgid: number }
export interface LogChunk { text: string; offset: number; done: boolean; exit_code: number | null }
export interface RunStatus { done: boolean; exit_code: number | null; changed_files: number; verdict: string }
