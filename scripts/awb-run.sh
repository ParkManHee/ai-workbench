#!/bin/sh
CLAUDE="$1"; DIR="$2"; LOG="$3"; SETTINGS="$4"; PLAN="$5"; PROMPT="$6"
cd "$DIR" 2>/dev/null || { echo "127" > "$LOG.done"; exit 127; }
if [ "$PLAN" = "1" ]; then
  "$CLAUDE" -p "$PROMPT" --settings "$SETTINGS" --permission-mode plan > "$LOG" 2>&1
else
  "$CLAUDE" -p "$PROMPT" --settings "$SETTINGS" > "$LOG" 2>&1
fi
echo "$?" > "$LOG.done"
