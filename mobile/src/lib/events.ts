import type { ChatState, WsEvent } from "./types";
export function initialChatState(): ChatState { return { messages: [], running: true, verdict: null, changedFiles: 0, error: null }; }
export function verdictLabel(v: string): string {
  if (v.startsWith("success")) return v.includes("무변경") ? "✅ 완료(변경 없음)" : "✅ 완료";
  if (v === "failed") return "❌ 실패";
  return "⏳ 실행 중";
}
/** 완료 카드 개수 — 턴 스냅샷 diff 가 있으면 그 아래 diff 박스와 같은 "이번 턴" 범위, 없으면 작업트리 전체(git status) */
export function changedFilesLabel(changedFiles: number, diff: { files: number; run?: string } | null): string {
  return diff?.run ? `이번 턴 변경 ${diff.files}개` : `변경 파일 ${changedFiles}개`;
}
export function reduceEvent(state: ChatState, ev: WsEvent): ChatState {
  const s: ChatState = { ...state, messages: [...state.messages] };
  const last = s.messages.at(-1);
  const ensureAssistant = () => {
    if (!last || last.role !== "assistant") { const m = { role: "assistant" as const, text: "", tools: [] as string[], toolDetails: [] as string[] }; s.messages.push(m); return m; }
    const m = { ...last, tools: [...(last.tools ?? [])], toolDetails: [...(last.toolDetails ?? [])] }; s.messages[s.messages.length - 1] = m; return m;
  };
  switch (ev.kind) {
    case "token": { const m = ensureAssistant(); m.text += ev.text; break; }
    case "tool_use": { const m = ensureAssistant(); m.tools!.push(ev.name); (m.toolDetails ??= []).push(ev.summary ? `${ev.name}: ${ev.summary}` : ev.name); break; }
    case "done": { s.running = false; s.verdict = ev.verdict; s.changedFiles = ev.changed_files; break; }
    case "error": { s.error = ev.message; s.running = false; break; }
    // 실행 중 할 일 바 — 바는 todos 가 있는 마지막 메시지를 그리므로 지금 말풍선에 싣는다
    case "todos": { s.messages[s.messages.length - 1] = { ...ensureAssistant(), todos: ev.todos }; break; }
  }
  return s;
}
