#!/bin/sh
# awb-run-stream.sh <claude> <dir> <log> <settings> <plan(0|1)> <resume(''|sid)> <prompt>
CLAUDE="$1"; DIR="$2"; LOG="$3"; SETTINGS="$4"; PLAN="$5"; RESUME="$6"; PROMPT="$7"
cd "$DIR" 2>/dev/null || { echo "127" > "$LOG.done"; exit 127; }
# --print(-p) + --output-format stream-json 는 --verbose 필수(claude 요구사항)
set -- "$CLAUDE" -p "$PROMPT" --settings "$SETTINGS" --output-format stream-json --verbose
[ "$PLAN" = "1" ] && set -- "$@" --permission-mode plan
[ -n "$RESUME" ] && set -- "$@" --resume "$RESUME"
"$@" > "$LOG" 2>&1
echo "$?" > "$LOG.done"
