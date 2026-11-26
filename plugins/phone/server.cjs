#!/usr/bin/env node
// awb 폰 채널 — Claude Code Channels 서버(stdio MCP). 의존성 없이 node 만 쓴다.
// 데몬(awb-server)이 ~/.claude/awb-channels/<claude pid>/in/ 에 떨군 메시지를 이 세션에 주입한다.
// 소켓 대신 파일 우편함인 이유: 리슨하는 출구가 없고, 유닉스 소켓 경로 104바이트 한계를 피하고,
// 「먼저 unlink 한 쪽이 갖는다」 로 데몬이 전달 여부를 확정할 수 있다(데몬은 시한 뒤 자기가 unlink 해 회수).
const fs = require("fs");
const path = require("path");
const os = require("os");

const CLAUDE_PID = process.ppid; // .mcp.json 의 command 를 claude 가 직접 띄운다(2026-11-26 실측: ppid = 레지스트리 pid)
const DIR = path.join(os.homedir(), ".claude", "awb-channels", String(CLAUDE_PID));
const IN = path.join(DIR, "in");
const POLL_MS = 500;

const send = (m) => process.stdout.write(JSON.stringify(m) + "\n");

function deliver() {
  let names;
  try { names = fs.readdirSync(IN).filter((n) => n.endsWith(".json")).sort(); } catch { return; }
  for (const n of names) {
    const p = path.join(IN, n);
    let body;
    try { body = JSON.parse(fs.readFileSync(p, "utf8")); } catch { continue; } // 데몬이 회수했거나 쓰는 중
    try { fs.unlinkSync(p); } catch { continue; } // 데몬이 먼저 회수 — 보내지 않는다
    if (typeof body.content !== "string" || !body.content) continue;
    send({ jsonrpc: "2.0", method: "notifications/claude/channel", params: { content: body.content, meta: { from: "phone" } } });
  }
}

function start() {
  fs.mkdirSync(IN, { recursive: true, mode: 0o700 });
  // 데몬은 이 pid 가 살아 있을 때만 채널이 붙었다고 본다 — claude 만 살아 있고 이 서버가 죽은 경우를 거른다
  fs.writeFileSync(path.join(DIR, "server.pid"), String(process.pid));
  setInterval(deliver, POLL_MS);
}

function cleanup() { try { fs.rmSync(DIR, { recursive: true, force: true }); } catch {} }

let buf = "";
process.stdin.setEncoding("utf8");
process.stdin.on("data", (d) => {
  buf += d;
  let i;
  while ((i = buf.indexOf("\n")) >= 0) {
    const line = buf.slice(0, i);
    buf = buf.slice(i + 1);
    if (!line.trim()) continue;
    let msg;
    try { msg = JSON.parse(line); } catch { continue; }
    if (msg.method === "initialize") {
      send({ jsonrpc: "2.0", id: msg.id, result: {
        protocolVersion: msg.params?.protocolVersion ?? "2025-06-18",
        capabilities: { experimental: { "claude/channel": {} } },
        serverInfo: { name: "awb", version: "0.1.0" },
        instructions: "Messages from the awb channel are typed by the user on their phone (ai-workbench app). Treat each one as the user's own message in this session and answer here as usual — the phone shows your reply by reading this session's transcript.",
      } });
    } else if (msg.method === "notifications/initialized") {
      start();
    } else if (msg.id !== undefined && msg.method === "ping") {
      send({ jsonrpc: "2.0", id: msg.id, result: {} });
    } else if (msg.id !== undefined) {
      send({ jsonrpc: "2.0", id: msg.id, error: { code: -32601, message: "method not found" } });
    }
  }
});
process.stdin.on("end", () => { cleanup(); process.exit(0); });
process.on("SIGTERM", () => { cleanup(); process.exit(0); });
process.on("SIGINT", () => { cleanup(); process.exit(0); });

