#!/bin/sh
# awb-run-stream.sh <claude> <dir> <log> <settings> <plan(0|1)> <resume(''|sid)> <prompt> [mcp_approval_cfg] [model] [effort] [fork(''|1)] [mode]
# 본문 전체를 { } 로 감싸 실행 전에 끝까지 파싱한다 — 실행 중 이 파일이 바뀌면 sh 가 바뀐 바이트 오프셋부터 이어 읽어 claude 를 한 번 더 돌린다(2026-09-23 실측)
{
CLAUDE="$1"; DIR="$2"; LOG="$3"; SETTINGS="$4"; PLAN="$5"; RESUME="$6"; PROMPT="$7"; MCPCFG="$8"; MODEL="$9"; EFFORT="${10}"; FORK="${11}"; MODE="${12}"
cd "$DIR" 2>/dev/null || { echo "127" > "$LOG.done"; exit 127; }
# --print(-p) + --output-format stream-json 는 --verbose 필수(claude 요구사항)
set -- "$CLAUDE" -p "$PROMPT" --output-format stream-json --verbose
if [ -n "$MCPCFG" ]; then
  # 사전 허용 설정을 싣지 않는다 — 그 allow 목록이 Bash·Edit 를 미리 허용해 감독 모드에서도 묻지 않게 된다
  set -- "$@" --permission-prompt-tool mcp__awb-approval__approval_prompt --mcp-config "$MCPCFG"
else
  # 중계 없음(전체 모드): 지금까지처럼 사전 허용 설정(deny 목록 포함)을 싣는다
  set -- "$@" --settings "$SETTINGS"
fi
[ -n "$MODEL" ] && set -- "$@" --model "$MODEL"
[ -n "$EFFORT" ] && set -- "$@" --effort "$EFFORT"
# plan 이 켜지면 plan 이 우선한다. 모드는 늘 명시한다 — 생략하면 사용자 전역 설정의 defaultMode 를 따른다
if [ "$PLAN" = "1" ]; then set -- "$@" --permission-mode plan
elif [ -n "$MODE" ]; then set -- "$@" --permission-mode "$MODE"; fi
[ -n "$RESUME" ] && set -- "$@" --resume "$RESUME"
# 터미널에 열린 세션은 갈라서 새 ID 로 쓴다 — 같은 파일에 두 프로세스가 번갈아 쓰면 서로의 턴을 모른다
[ -n "$RESUME" ] && [ "$FORK" = "1" ] && set -- "$@" --fork-session
# 할 일 도구(TaskCreate 등)는 -p 에서 기본으로 꺼져 있다 — 앱 할 일 바·진행 알림이 그 목록을 읽는다
export CLAUDE_CODE_ENABLE_TODO_TOOLS=1
"$@" > "$LOG" 2>&1
echo "$?" > "$LOG.done"
exit
}
