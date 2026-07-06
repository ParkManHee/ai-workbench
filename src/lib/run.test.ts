import { describe, it, expect } from "vitest";
import { appendChunk, verdictLabel } from "./run";
describe("run", () => {
  it("appendChunk 누적", () => {
    const s = appendChunk({ text:"a", offset:1 }, { text:"b", offset:2, done:false, exit_code:null });
    expect(s.text).toBe("ab"); expect(s.offset).toBe(2);
  });
  it("verdictLabel", () => {
    expect(verdictLabel("failed")).toMatch(/실패/);
    expect(verdictLabel("success")).toMatch(/완료/);
  });
});
