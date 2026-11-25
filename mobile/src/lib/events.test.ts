import { describe, it, expect } from "vitest";
import { changedFilesLabel, initialChatState, reduceEvent, verdictLabel } from "./events";
describe("reduceEvent", () => {
  it("accumulates tokens into current assistant message", () => {
    let s = initialChatState();
    s = reduceEvent(s, { kind: "token", text: "안" });
    s = reduceEvent(s, { kind: "token", text: "녕" });
    expect(s.messages.at(-1)).toMatchObject({ role: "assistant", text: "안녕" });
    expect(s.running).toBe(true);
  });
  it("done sets verdict and stops running", () => {
    let s = reduceEvent(initialChatState(), { kind: "done", exit: 0, verdict: "success", changed_files: 2 });
    expect(s.running).toBe(false); expect(s.verdict).toBe("success");
  });
});
describe("verdictLabel", () => {
  it("labels", () => {
    expect(verdictLabel("success")).toMatch(/완료/);
    expect(verdictLabel("failed")).toMatch(/실패/);
    expect(verdictLabel("success(무변경)")).toMatch(/변경 없음/);
  });
});
describe("changedFilesLabel", () => {
  it("턴 diff 가 있으면 그 개수, 없으면 작업트리 전체 개수", () => {
    expect(changedFilesLabel(1, { files: 0, run: "r1" })).toBe("이번 턴 변경 0개");
    expect(changedFilesLabel(1, { files: 1 })).toBe("변경 파일 1개");
    expect(changedFilesLabel(1, null)).toBe("변경 파일 1개");
  });
});
