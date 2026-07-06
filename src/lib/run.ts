export interface RunState { text: string; offset: number }
export interface LogChunkLike { text: string; offset: number; done?: boolean; exit_code?: number | null }
export function appendChunk(prev: RunState, chunk: LogChunkLike): RunState {
  return { text: prev.text + chunk.text, offset: chunk.offset };
}
export function verdictLabel(v: string): string {
  if (v.startsWith("success")) return v.includes("무변경") ? "✅ 완료(변경 없음)" : "✅ 완료";
  if (v === "failed") return "❌ 실패";
  if (v === "blocked") return "⛔ 차단";
  return "⏳ 실행 중";
}
