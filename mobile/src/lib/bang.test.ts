import { describe, it, expect } from "vitest";
import { rewriteBangCommand } from "./bang";
describe("rewriteBangCommand", () => {
  it("! 접두는 실행 프롬프트로 감싸고, 그 외는 원문 그대로", () => {
    expect(rewriteBangCommand("!ls -la")).toContain("```\nls -la\n```");
    expect(rewriteBangCommand("ls -la")).toBe("ls -la");
  });
});
