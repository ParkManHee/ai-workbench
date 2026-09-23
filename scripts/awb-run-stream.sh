#!/bin/sh
# awb-run-stream.sh <claude> <dir> <log> <settings> <plan(0|1)> <resume(''|sid)> <prompt> [mcp_approval_cfg] [model] [effort] [fork(''|1)]
# 본문 전체를 { } 로 감싸 실행 전에 끝까지 파싱한다 — 실행 중 이 파일이 바뀌면 sh 가 바뀐 바이트 오프셋부터 이어 읽어 claude 를 한 번 더 돌린다(2026-09-23 실측)
{
CLAUDE="$1"; DIR="$2"; LOG="$3"; SETTINGS="$4"; PLAN="$5"; RESUME="$6"; PROMPT="$7"; MCPCFG="$8"; MODEL="$9"; EFFORT="${10}"; FORK="${11}"
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
[ -n "$EFFORT" ] && set -- "$@" --effort "$EFFORT"
[ "$PLAN" = "1" ] && set -- "$@" --permission-mode plan
[ -n "$RESUME" ] && set -- "$@" --resume "$RESUME"
# 터미널에 열린 세션은 갈라서 새 ID 로 쓴다 — 같은 파일에 두 프로세스가 번갈아 쓰면 서로의 턴을 모른다
[ -n "$RESUME" ] && [ "$FORK" = "1" ] && set -- "$@" --fork-session
"$@" > "$LOG" 2>&1
echo "$?" > "$LOG.done"
exit
}
