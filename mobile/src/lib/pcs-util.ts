// 순수 로직만 담는다(RN/Expo import 금지) — vitest에서 그대로 테스트 가능해야 한다.
import type { PC } from "./types";

/** baseUrl("http://host:port")에서 안정적인 id를 파생한다: 스킴 제거 후 비-영숫자를 '-'로 치환. */
export function pcId(baseUrl: string): string {
  const noScheme = baseUrl.replace(/^[a-zA-Z]+:\/\//, "");
  return noScheme.replace(/[^a-zA-Z0-9]+/g, "-").replace(/^-+|-+$/g, "");
}

/** 같은 baseUrl의 PC가 있으면 교체(중복 방지), 없으면 목록 끝에 추가. 입력 배열은 변경하지 않는다. */
export function upsertPC(list: PC[], pc: PC): PC[] {
  const idx = list.findIndex((p) => p.baseUrl === pc.baseUrl);
  if (idx === -1) return [...list, pc];
  const next = [...list];
  next[idx] = pc;
  return next;
}

/** 온라인/오프라인 표시 문구. 오프라인이면 마지막 응답 시각(없으면 "응답 없음")을 붙인다. */
export function presenceLabel(online: boolean | undefined, lastSeen: number | undefined, now: number): string {
  // 이번 실행에서 아직 /health 결과가 없으면 저장된 lastSeen 으로 오프라인이라 단정하지 않는다
  if (online === undefined) return "확인 중…";
  if (online) return "🟢 온라인";
  if (lastSeen === undefined) return "⚫ 오프라인 · 응답 없음";
  const min = Math.floor(Math.max(0, now - lastSeen) / 60000);
  const ago = min < 1 ? "방금" : min < 60 ? `${min}분 전` : min < 1440 ? `${Math.floor(min / 60)}시간 전` : `${Math.floor(min / 1440)}일 전`;
  return `⚫ 오프라인 · 마지막 응답 ${ago}`;
}
