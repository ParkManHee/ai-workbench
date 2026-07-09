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
