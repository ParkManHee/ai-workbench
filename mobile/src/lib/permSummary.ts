// 순수 로직만 담는다(RN/Expo import 금지) — vitest 에서 그대로 테스트한다.

/** 권한 카드에 보일 모양 — 도구마다 사람이 읽을 부분만 */
export type PermView =
  | { kind: "file"; path: string; preview: string }
  | { kind: "command"; command: string; description: string }
  | { kind: "plan"; plan: string }
  | { kind: "question"; questions: { question: string; options: string[] }[] }
  | { kind: "raw" };

/**
 * 데몬은 도구 입력 JSON 을 300자에서 잘라 「…」 를 붙여 보낸다(routes.rs permission_request_handler) —
 * 긴 Write·계획은 JSON.parse 가 실패하므로, 키로 문자열 값을 직접 읽어 잘린 앞부분이라도 보인다.
 */
export function stringValues(raw: string, key: string): string[] {
  const out: string[] = [];
  const re = new RegExp(`"${key}"\\s*:\\s*"`, "g");
  let m: RegExpExecArray | null;
  while ((m = re.exec(raw))) {
    let i = re.lastIndex;
    let body = "";
    while (i < raw.length && raw[i] !== '"') {
      body += raw[i] === "\\" ? raw.slice(i, i + 2) : raw[i];
      i += raw[i] === "\\" ? 2 : 1;
    }
    // 잘린 끝의 반쪽 이스케이프(\ · \u12)는 버린다
    body = body.replace(/\\(u[0-9a-fA-F]{0,3})?$/, "");
    try {
      out.push(JSON.parse(`"${body}"`));
    } catch {
      out.push(body);
    }
    re.lastIndex = i;
  }
  return out;
}

export function permView(tool: string, summary: string): PermView {
  const first = (k: string) => stringValues(summary, k)[0];
  switch (tool) {
    case "Write":
    case "Edit":
    case "MultiEdit":
    case "NotebookEdit": {
      const path = first("file_path") ?? first("notebook_path");
      if (path === undefined) break;
      return { kind: "file", path, preview: first("content") ?? first("new_string") ?? first("new_source") ?? "" };
    }
    case "Bash": {
      const command = first("command");
      if (command === undefined) break;
      return { kind: "command", command, description: first("description") ?? "" };
    }
    case "ExitPlanMode": {
      const plan = first("plan");
      if (plan === undefined) break;
      return { kind: "plan", plan };
    }
    case "AskUserQuestion": {
      // 질문마다 뒤따르는 label 이 그 질문의 선택지다 — 잘린 JSON 이라 구조 대신 등장 순서로 묶는다
      const questions = summary.split(/(?="question"\s*:)/).slice(1).map((seg) => ({
        question: stringValues(seg, "question")[0] ?? "",
        options: stringValues(seg, "label"),
      }));
      if (questions.length === 0) break;
      return { kind: "question", questions };
    }
  }
  return { kind: "raw" };
}
