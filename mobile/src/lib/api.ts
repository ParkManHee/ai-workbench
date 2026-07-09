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
    diffFile: (path: string, file: string): Promise<{ file: string; diff: string }> =>
      jget(`/diff?path=${encodeURIComponent(path)}&file=${encodeURIComponent(file)}`),
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
    /** 첨부 이미지 업로드(base64 → raw bytes) → Mac 저장 절대경로를 반환받는다.
     * expo 전역 fetch(WinterCG)는 RN식 {uri,...} FormData 파트를 지원하지 않고
     * ("unsupported FormData part implementation"), 로컬 uri의 blob() 읽기도 안 되므로
     * 픽커에서 받은 base64를 Uint8Array로 디코드해 본문으로 보낸다. */
    upload: async (base64: string, ext: string): Promise<{ path: string }> => {
      // Android의 Base64 인코더는 76자마다 줄바꿈을 넣을 수 있고, Hermes atob는
      // 공백이 섞이면 "not a valid base64 encoded string length"로 거부한다 — 제거 후 디코드.
      const bin = atob(base64.replace(/[\r\n\s]/g, ""));
      const bytes = new Uint8Array(bin.length);
      for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
      const r = await f(`${baseUrl}/upload?ext=${encodeURIComponent(ext)}`, {
        method: "POST",
        headers: { ...h, "Content-Type": "application/octet-stream" },
        body: bytes,
      } as any);
      if (!(r as any).ok) throw new HttpError((r as any).status, "/upload");
      return (r as any).json();
    },
    registerPush: (pushToken: string) => f(`${baseUrl}/push/register`, { method: "POST", headers: { ...h, "Content-Type": "application/json" }, body: JSON.stringify({ token: pushToken }) } as any),
  };
}
