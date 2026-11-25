import type { SessionInfo } from "./types";

/** 활성 세션 — 터미널 열림·실행 중·응답 대기. 세션 화면과 프로젝트 화면이 같은 기준을 쓴다 */
export const isActiveSession = (s: SessionInfo) => !!(s.live || s.active || s.waiting);

/** 대략적인 상대 시간(분/시간/일) 표시, 오래된 항목은 날짜로. unix seconds 입력. */
export function formatUpdated(unixSeconds: number): string {
  const diffMs = Date.now() - unixSeconds * 1000;
  const diffMin = Math.floor(diffMs / 60000);
  if (diffMin < 1) return "방금 전";
  if (diffMin < 60) return `${diffMin}분 전`;
  const diffHour = Math.floor(diffMin / 60);
  if (diffHour < 24) return `${diffHour}시간 전`;
  const diffDay = Math.floor(diffHour / 24);
  if (diffDay < 7) return `${diffDay}일 전`;
  return new Date(unixSeconds * 1000).toLocaleDateString();
}
