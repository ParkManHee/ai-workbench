#!/bin/sh
# awb-run-stream.sh <claude> <dir> <log> <settings> <plan(0|1)> <resume(''|sid)> <prompt> [mcp_approval_cfg]
CLAUDE="$1"; DIR="$2"; LOG="$3"; SETTINGS="$4"; PLAN="$5"; RESUME="$6"; PROMPT="$7"; MCPCFG="$8"; MODEL="$9"
cd "$DIR" 2>/dev/null || { echo "127" > "$LOG.done"; exit 127; }
# --print(-p) + --output-format stream-json 는 --verbose 필수(claude 요구사항)
set -- "$CLAUDE" -p "$PROMPT" --output-format stream-json --verbose
if [ -n "$MCPCFG" ]; then
  # 승인 모드: 사전 허용 설정 대신 권한 프롬프트를 폰으로 릴레이(MCP)
  set -- "$@" --permission-prompt-tool mcp__awb-approval__approval_prompt --mcp-config "$MCPCFG"
else
  set -- "$@" --settings "$SETTINGS"
fi
[ -n "$MODEL" ] && set -- "$@" --model "$MODEL"
[ "$PLAN" = "1" ] && set -- "$@" --permission-mode plan
[ -n "$RESUME" ] && set -- "$@" --resume "$RESUME"
"$@" > "$LOG" 2>&1
echo "$?" > "$LOG.done"
