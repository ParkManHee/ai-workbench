export interface Project { name: string; path: string; has_origin: boolean; last_activity: number }
export interface Check { id: string; ok: boolean; detail: string }
export interface Preflight { claude_path: string | null; checks: Check[] }
export interface Badge { todo: number; doing: number; done: number; updated: string }
