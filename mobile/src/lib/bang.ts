/** `!cmd` 입력 → claude 에게 그대로 실행시키는 프롬프트. 데몬에 셸 엔드포인트가 없어 /chat 경로로 보낸다. */
export function rewriteBangCommand(text: string): string {
  const t = text.trim();
  if (!t.startsWith("!")) return text;
  const cmd = t.slice(1).trim();
  if (!cmd) return text;
  return `다음 셸 명령을 Bash 도구로 **그대로** 실행하고 출력(stdout·stderr·종료코드)을 코드블록으로만 보여줘. 설명·요약·수정 제안 없이.\n\n\`\`\`\n${cmd}\n\`\`\``;
}
