// 순수 로직만 담는다(RN/Expo import 금지) — vitest에서 그대로 테스트 가능해야 한다.

export const DAYS = ["일", "월", "화", "수", "목", "금", "토"];

/** 예약 다음 회차 표시 — 요일만 쓰면 오늘 이미 지난 시각의 7일 뒤 회차가 「오늘」 로 읽힌다. 오늘·내일만 말로, 그 밖은 날짜를 붙인다 */
export function nextLabel(epoch: number, now: Date = new Date()): string {
  const d = new Date(epoch * 1000);
  const hm = `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
  const today = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  const tomorrow = new Date(now.getFullYear(), now.getMonth(), now.getDate() + 1);
  const day = new Date(d.getFullYear(), d.getMonth(), d.getDate());
  if (day.getTime() === today.getTime()) return `오늘 ${hm}`;
  if (day.getTime() === tomorrow.getTime()) return `내일 ${hm}`;
  return `${d.getMonth() + 1}/${d.getDate()}(${DAYS[d.getDay()]}) ${hm}`;
}
