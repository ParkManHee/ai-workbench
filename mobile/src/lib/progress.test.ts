import { describe, it, expect } from "vitest";
import { progressAction } from "./progress";
describe("progressAction", () => {
  it("progress 는 프로젝트 고정 identifier 로 단계·todo 를 표시하고, progress_end 는 같은 identifier 를 지운다", () => {
    expect(progressAction({ kind: "progress", project: "awb", step: "Bash", todo: { done: 2, total: 5 } })).toEqual({
      kind: "show",
      identifier: "progress-awb",
      title: "⏳ awb · Bash (todo 2/5)",
    });
    expect(progressAction({ kind: "progress", project: "awb", step: "Read", todo: null })).toMatchObject({ title: "⏳ awb · Read" });
    expect(progressAction({ kind: "progress_end", project: "awb", run_id: "r" })).toEqual({
      kind: "dismiss",
      identifier: "progress-awb",
    });
  });
});
