#!/bin/sh
# 본문 전체를 { } 로 감싸 실행 전에 끝까지 파싱한다 — 실행 중 이 파일이 바뀌면 sh 가 바뀐 바이트 오프셋부터 이어 읽어 claude 를 한 번 더 돌린다(2026-09-23 실측)
{
CLAUDE="$1"; DIR="$2"; LOG="$3"; SETTINGS="$4"; PLAN="$5"; PROMPT="$6"
cd "$DIR" 2>/dev/null || { echo "127" > "$LOG.done"; exit 127; }
if [ "$PLAN" = "1" ]; then
  "$CLAUDE" -p "$PROMPT" --settings "$SETTINGS" --permission-mode plan > "$LOG" 2>&1
else
  "$CLAUDE" -p "$PROMPT" --settings "$SETTINGS" > "$LOG" 2>&1
fi
echo "$?" > "$LOG.done"
exit
}
