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
    chat: async (project: string, prompt: string, plan: boolean, resumeSessionId?: string) => {
      const body: Record<string, unknown> = { prompt, plan };
      if (resumeSessionId) body.resume_session_id = resumeSessionId;
      const r = await f(`${baseUrl}/chat/${encodeURIComponent(project)}`, { method: "POST", headers: { ...h, "Content-Type": "application/json" }, body: JSON.stringify(body) } as any);
      if (!(r as any).ok) throw new HttpError((r as any).status, `/chat/${project}`);
      return (r as any).json() as Promise<{ run_id: string; log: string }>;
    },
    cancel: (runId: string) => f(`${baseUrl}/cancel/${runId}`, { method: "POST", headers: h } as any),
    registerPush: (pushToken: string) => f(`${baseUrl}/push/register`, { method: "POST", headers: { ...h, "Content-Type": "application/json" }, body: JSON.stringify({ token: pushToken }) } as any),
  };
}
