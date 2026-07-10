#!/usr/bin/env node
// 권한 승인 릴레이 MCP 서버(stdio, newline-delimited JSON-RPC).
// claude가 --permission-prompt-tool mcp__awb-approval__approval_prompt 로 호출하면
// 데몬 /permission/request 에 전달하고(폰 응답까지 블로킹), allow/deny를 돌려준다.
// env: AWB_DAEMON(데몬 base url), AWB_PERM_SECRET(내부 시크릿), AWB_PROJECT(프로젝트명)
const readline = require("readline");

const DAEMON = process.env.AWB_DAEMON || "";
const SECRET = process.env.AWB_PERM_SECRET || "";
const PROJECT = process.env.AWB_PROJECT || "";

const send = (obj) => process.stdout.write(JSON.stringify(obj) + "\n");

const rl = readline.createInterface({ input: process.stdin });
rl.on("line", async (line) => {
  let msg;
  try { msg = JSON.parse(line); } catch { return; }
  const { id, method, params } = msg;

  if (method === "initialize") {
    send({ jsonrpc: "2.0", id, result: {
      protocolVersion: (params && params.protocolVersion) || "2024-11-05",
      capabilities: { tools: {} },
      serverInfo: { name: "awb-approval", version: "1.0.0" },
    }});
    return;
  }
  if (method === "notifications/initialized") return; // 알림 — 응답 없음
  if (method === "tools/list") {
    send({ jsonrpc: "2.0", id, result: { tools: [{
      name: "approval_prompt",
      description: "폰으로 툴 사용 승인을 요청한다",
      inputSchema: {
        type: "object",
        properties: {
          tool_name: { type: "string" },
          input: { type: "object" },
          tool_use_id: { type: "string" },
        },
        required: ["tool_name", "input"],
      },
    }]}});
    return;
  }
  if (method === "tools/call") {
    const args = (params && params.arguments) || {};
    // 기본은 거부 — 데몬/폰에 도달 못 하면 위험한 툴을 자동 허용하지 않는다
    let decision = { behavior: "deny", message: "승인 요청 전달 실패 또는 시간 초과" };
    try {
      const res = await fetch(`${DAEMON}/permission/request`, {
        method: "POST",
        headers: { "Content-Type": "application/json", Authorization: `Bearer ${SECRET}` },
        body: JSON.stringify({ project: PROJECT, tool_name: args.tool_name || "?", input: args.input || {} }),
      });
      if (res.ok) {
        const j = await res.json();
        decision = j.allow
          ? { behavior: "allow", updatedInput: args.input || {} }
          : { behavior: "deny", message: "폰에서 거부되었습니다" };
      }
    } catch (_) { /* 기본 deny 유지 */ }
    send({ jsonrpc: "2.0", id, result: { content: [{ type: "text", text: JSON.stringify(decision) }] } });
    return;
  }
  if (id !== undefined) {
    send({ jsonrpc: "2.0", id, error: { code: -32601, message: `method not found: ${method}` } });
  }
});
