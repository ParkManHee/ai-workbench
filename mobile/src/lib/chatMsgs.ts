import type { ChatMsg } from "./types";

const toolOnly = (m: ChatMsg) => m.role === "assistant" && !!m.tools?.length && !m.text.trim();
const detailsOf = (m: ChatMsg) => (m.toolDetails?.length ? m.toolDetails : m.tools ?? []);

/** 연이은 「도구만 있는」 assistant 메시지를 칩 하나로 묶는다 — Claude Code 는 도구 호출마다 기록 줄을 따로 남겨
 *  묶지 않으면 「작업 1」 칩이 줄줄이 선다. key 는 묶음 첫 메시지 index: 뒤에 이어 붙어도 그대로라 펼침 상태가 유지된다 */
export function groupToolRuns(msgs: ChatMsg[]): { key: number; msg: ChatMsg }[] {
  const out: { key: number; msg: ChatMsg }[] = [];
  msgs.forEach((m, i) => {
    const last = out[out.length - 1];
    if (last && toolOnly(m) && toolOnly(last.msg)) {
      last.msg = { ...last.msg, tools: [...last.msg.tools!, ...m.tools!], toolDetails: [...detailsOf(last.msg), ...detailsOf(m)] };
    } else out.push({ key: i, msg: m });
  });
  return out;
}

/** 폴링으로 받은 새 메시지를 붙인다. 채널 전송 때 먼저 붙인 말풍선(pending)은 대화 기록의 사용자 메시지가 오면 뺀다 —
 *  안 빼면 같은 말풍선이 둘 섰다가 재로드 때 하나가 사라진다 */
export function appendPolled(prev: ChatMsg[], incoming: ChatMsg[]): ChatMsg[] {
  const base = incoming.some((m) => m.role === "user") ? prev.filter((m) => !m.pending) : prev;
  return [...base, ...incoming];
}
