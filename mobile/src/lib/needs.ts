import { HttpError, makeClient } from "./api";
import type { NeedsYou, PC } from "./types";

/** 홈 「답 필요」 갱신 주기. 홈에는 원래 주기 확인이 없어 새로 정했다 — 권한 요청은 데몬이 기다리는 동안만 살아 있어 분 단위는 늦고,
 * /needs-you 는 /projects 만큼 트랜스크립트를 읽어 채팅 화면의 2.5초는 과하다 */
export const NEEDS_POLL_MS = 15000;
/** 꺼진 PC 의 fetch 는 오래 매달린다 — 그 PC 를 「응답 없음」 으로 두고 나머지를 먼저 보인다 */
const TIMEOUT_MS = 8000;

/** data 가 null 이면 그 PC 는 목록에서 빠졌다 — offline(응답 없음) | old(이 API 가 없는 옛 데몬) */
export type PcNeeds = { pc: PC; data: NeedsYou | null; reason?: "offline" | "old" };

export async function fetchNeeds(pcs: PC[]): Promise<PcNeeds[]> {
  return Promise.all(pcs.map(async (pc): Promise<PcNeeds> => {
    let timer: ReturnType<typeof setTimeout> | undefined;
    try {
      const timeout = new Promise<never>((_, rej) => { timer = setTimeout(() => rej(new Error("timeout")), TIMEOUT_MS); });
      return { pc, data: await Promise.race([makeClient(pc.baseUrl, pc.token).needsYou(), timeout]) };
    } catch (e) {
      return { pc, data: null, reason: e instanceof HttpError && e.status === 404 ? "old" : "offline" };
    } finally {
      clearTimeout(timer);
    }
  }));
}

export const needsCount = (list: PcNeeds[]) =>
  list.reduce((n, x) => n + (x.data ? x.data.permissions.length + x.data.waiting.length : 0), 0);
