import type { Project, Preflight, SessionInfo, TranscriptMsg } from "./types";

/** HTTP error carrying the response status so callers can act on 401 (revoked token). */
export class HttpError extends Error {
  status: number;
  constructor(status: number, path: string) {
    super(`${path} ${status}`);
    this.name = "HttpError";
    this.status = status;
  }
}
/** True when an error is a 401 Unauthorized (token missing/revoked) → caller should re-pair. */
export function isUnauthorized(e: unknown): boolean {
  return e instanceof HttpError && e.status === 401;
}

export function pairUrl(baseUrl: string, code: string) { return `${baseUrl}/pair?code=${encodeURIComponent(code)}`; }
export function streamUrl(baseUrl: string, runId: string, offset: number, token: string) {
  const ws = baseUrl.replace(/^http/, "ws");
  return `${ws}/stream/${runId}?offset=${offset}&token=${encodeURIComponent(token)}`;
}
type F = typeof fetch;
export function makeClient(baseUrl: string, token: string, f: F = fetch) {
  const h = { Authorization: `Bearer ${token}` };
  const jget = async (p: string) => { const r = await f(`${baseUrl}${p}`, { headers: h } as any); if (!(r as any).ok) throw new HttpError((r as any).status, p); return (r as any).json(); };
  return {
    projects: (): Promise<Project[]> => jget("/projects"),
    preflight: (): Promise<Preflight> => jget("/preflight"),
    diff: (path: string) => jget(`/diff?path=${encodeURIComponent(path)}`),
    status: (runId: string) => jget(`/status/${runId}`),
    info: (): Promise<{ hostname: string }> => jget("/info"),
    sessions: (project: string): Promise<SessionInfo[]> => jget(`/sessions/${encodeURIComponent(project)}`),
    transcript: (project: string, sessionId: string, from = 0): Promise<{ messages: TranscriptMsg[]; next: number; active: boolean }> =>
      jget(`/transcript/${encodeURIComponent(project)}/${encodeURIComponent(sessionId)}?from=${from}`),
    transcriptTail: (project: string, sessionId: string): Promise<{ messages: TranscriptMsg[]; next: number; active: boolean; prev: number | null }> =>
      jget(`/transcript/${encodeURIComponent(project)}/${encodeURIComponent(sessionId)}?tail=1`),
    transcriptBefore: (project: string, sessionId: string, until: number): Promise<{ messages: TranscriptMsg[]; next: number; active: boolean; prev: number | null }> =>
      jget(`/transcript/${encodeURIComponent(project)}/${encodeURIComponent(sessionId)}?until=${until}&limit=50`),
    chat: async (project: string, prompt: string, plan: boolean, resumeSessionId?: string) => {
      const body: Record<string, unknown> = { prompt, plan };
      if (resumeSessionId) body.resume_session_id = resumeSessionId;
      const r = await f(`${baseUrl}/chat/${encodeURIComponent(project)}`, { method: "POST", headers: { ...h, "Content-Type": "application/json" }, body: JSON.stringify(body) } as any);
      if (!(r as any).ok) throw new HttpError((r as any).status, `/chat/${project}`);
      return (r as any).json() as Promise<{ run_id: string; log: string }>;
    },
    cancel: (runId: string) => f(`${baseUrl}/cancel/${runId}`, { method: "POST", headers: h } as any),
    /** 첨부 이미지 업로드 → Mac 저장 절대경로를 반환받는다.
     * RN의 fetch는 로컬 content://·file:// uri를 JS에서 읽지 못하므로(blob() 실패),
     * FormData {uri,name,type} 파트로 넘겨 네이티브가 파일을 직접 스트리밍하게 한다. */
    upload: async (uri: string, ext: string): Promise<{ path: string }> => {
      const form = new FormData();
      const mime = ext === "jpg" ? "jpeg" : ext;
      form.append("file", { uri, name: `image.${ext}`, type: `image/${mime}` } as any);
      const r = await f(`${baseUrl}/upload?ext=${encodeURIComponent(ext)}`, {
        method: "POST",
        headers: h, // Content-Type은 RN이 boundary 포함해 자동 설정
        body: form,
      } as any);
      if (!(r as any).ok) throw new HttpError((r as any).status, "/upload");
      return (r as any).json();
    },
    registerPush: (pushToken: string) => f(`${baseUrl}/push/register`, { method: "POST", headers: { ...h, "Content-Type": "application/json" }, body: JSON.stringify({ token: pushToken }) } as any),
  };
}
