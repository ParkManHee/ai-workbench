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
