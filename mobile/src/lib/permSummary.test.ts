import { describe, it, expect } from "vitest";
import { permView } from "./permSummary";

// 데몬처럼 300자에서 자르고 「…」 — 실제로 오는 모양
const cut = (input: unknown) => {
  const raw = JSON.stringify(input);
  return raw.length > 300 ? `${raw.slice(0, 300)}…` : raw;
};

describe("permView", () => {
  it("잘린 Write 입력에서도 경로와 내용 앞부분을 읽는다(잘린 자리의 「…」 는 남긴다)", () => {
    const v = permView("Write", cut({ file_path: "/a/b.md", content: `첫 줄 "인용"\n${"x".repeat(400)}` }));
    expect(v).toEqual({ kind: "file", path: "/a/b.md", preview: expect.stringMatching(/^첫 줄 "인용"\nx+…$/) });
  });

  it("잘린 계획 본문 — 끝의 반쪽 이스케이프는 버린다", () => {
    const s = `${cut({ plan: `# 계획\n${"y".repeat(400)}` }).slice(0, 20)}\\`;
    expect(permView("ExitPlanMode", s)).toEqual({ kind: "plan", plan: "# 계획\nyyyyy" });
  });

  it("AskUserQuestion 은 질문마다 자기 선택지만", () => {
    const v = permView("AskUserQuestion", cut({ questions: [
      { question: "Q1?", header: "h", options: [{ label: "A", description: "d" }, { label: "B", description: "d" }], multiSelect: false },
      { question: "Q2?", header: "h", options: [{ label: "C", description: "d" }], multiSelect: false },
    ] }));
    expect(v).toEqual({ kind: "question", questions: [{ question: "Q1?", options: ["A", "B"] }, { question: "Q2?", options: ["C"] }] });
  });

  it("모르는 도구·기대 키 없음은 raw", () => {
    expect(permView("WebFetch", cut({ url: "u" }))).toEqual({ kind: "raw" });
    expect(permView("Bash", cut({ cmd: "ls" }))).toEqual({ kind: "raw" });
  });
});
