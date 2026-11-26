import { describe, it, expect } from "vitest";
import { appendPolled, groupToolRuns } from "./chatMsgs";
import type { ChatMsg } from "./types";

const tool = (n: string, d?: string): ChatMsg => ({ role: "assistant", text: "", tools: [n], toolDetails: d ? [d] : undefined });

describe("groupToolRuns", () => {
  it("연이은 도구 칩은 하나로 묶고, 답변·사용자 말풍선에서 끊는다", () => {
    const g = groupToolRuns([
      { role: "user", text: "고쳐" },
      tool("REPL", "REPL: a"), tool("Poll"), tool("REPL", "REPL: b"),
      { role: "assistant", text: "원인은…" },
      tool("REPL", "REPL: c"),
    ]);
    expect(g.map((x) => x.key)).toEqual([0, 1, 4, 5]);
    expect(g[1].msg.tools).toEqual(["REPL", "Poll", "REPL"]);
    expect(g[1].msg.toolDetails).toEqual(["REPL: a", "Poll", "REPL: b"]);
    expect(g[3].msg.tools).toEqual(["REPL"]);
  });
});

describe("appendPolled", () => {
  it("채널로 먼저 붙인 말풍선은 대화 기록의 사용자 메시지가 오면 빠진다", () => {
    const prev: ChatMsg[] = [{ role: "assistant", text: "앞 답변" }, { role: "user", text: "보냄", pending: true }];
    expect(appendPolled(prev, [tool("REPL")])).toHaveLength(3); // 아직 안 왔으면 유지
    expect(appendPolled(prev, [{ role: "user", text: "보냄" }])).toEqual([{ role: "assistant", text: "앞 답변" }, { role: "user", text: "보냄" }]);
  });
});
