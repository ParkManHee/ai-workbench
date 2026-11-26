import type { SessionInfo, WorktreeCleanup } from "./types";

/** 활성 세션 — 터미널 열림·실행 중·응답 대기. 세션 화면과 프로젝트 화면이 같은 기준을 쓴다 */
export const isActiveSession = (s: SessionInfo) => !!(s.live || s.active || s.waiting);

/** 대화 화면 경로 인자 — 격리 세션은 그 트리 경로와 브랜치(변경 보기·이어받기 명령·헤더). 정리된 트리는 원래 트리.
 * 채널이 붙은 터미널 세션이면 channel=1(대화 화면이 이어받기·fork 안내 대신 「터미널 연결됨」 을 보인다) */
export function sessionPathParams(s: SessionInfo, path: string): { path: string; branch?: string; channel?: string } {
  const p = s.worktree && !s.worktree.removed ? { path: s.worktree.path, branch: s.worktree.branch } : { path };
  return s.channel ? { ...p, channel: "1" } : p;
}

/** 끝내기·보관 뒤 격리 트리 정리 결과 한 줄 */
export function cleanupMessage(c: WorktreeCleanup): string {
  if (!c.removed) return `격리 트리를 남겼습니다 — ${c.reason ?? "이유 모름"}`;
  return c.branch_deleted ? `격리 트리와 브랜치 ${c.branch} 를 지웠습니다` : `격리 트리를 지웠습니다 — ${c.reason ?? `브랜치 ${c.branch} 는 남김`}`;
}

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
