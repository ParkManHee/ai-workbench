export const PROGRESS_CHANNEL = "progress";

export type ProgressAction =
  | { kind: "show"; identifier: string; title: string }
  | { kind: "dismiss"; identifier: string };

/** 데몬 data-only 푸시(progress / progress_end) → 알림 동작. 프로젝트마다 한 장을 덮어쓰려고 identifier 를 고정한다. */
export function progressAction(p: Record<string, unknown>): ProgressAction | null {
  if (typeof p.project !== "string") return null;
  const identifier = `progress-${p.project}`;
  if (p.kind === "progress_end") return { kind: "dismiss", identifier };
  if (p.kind !== "progress" || typeof p.step !== "string") return null;
  const todo = p.todo as { done?: unknown; total?: unknown } | null | undefined;
  const suffix =
    typeof todo?.done === "number" && typeof todo?.total === "number" ? ` (할 일 ${todo.done}/${todo.total})` : "";
  return { kind: "show", identifier, title: `⏳ ${p.project} · ${p.step}${suffix}` };
}
