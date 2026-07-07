import type { Project, Preflight } from "./types";
export function pairUrl(baseUrl: string, code: string) { return `${baseUrl}/pair?code=${encodeURIComponent(code)}`; }
export function streamUrl(baseUrl: string, runId: string, offset: number, token: string) {
  const ws = baseUrl.replace(/^http/, "ws");
  return `${ws}/stream/${runId}?offset=${offset}&token=${encodeURIComponent(token)}`;
}
type F = typeof fetch;
export function makeClient(baseUrl: string, token: string, f: F = fetch) {
  const h = { Authorization: `Bearer ${token}` };
  const jget = async (p: string) => { const r = await f(`${baseUrl}${p}`, { headers: h } as any); if (!(r as any).ok) throw new Error(`${p} ${(r as any).status}`); return (r as any).json(); };
  return {
    projects: (): Promise<Project[]> => jget("/projects"),
    preflight: (): Promise<Preflight> => jget("/preflight"),
    diff: (path: string) => jget(`/diff?path=${encodeURIComponent(path)}`),
    status: (runId: string) => jget(`/status/${runId}`),
    chat: async (project: string, prompt: string, plan: boolean) => {
      const r = await f(`${baseUrl}/chat/${encodeURIComponent(project)}`, { method: "POST", headers: { ...h, "Content-Type": "application/json" }, body: JSON.stringify({ prompt, plan }) } as any);
      if (!(r as any).ok) throw new Error(`chat ${(r as any).status}`);
      return (r as any).json() as Promise<{ run_id: string; log: string }>;
    },
    cancel: (runId: string) => f(`${baseUrl}/cancel/${runId}`, { method: "POST", headers: h } as any),
    registerPush: (pushToken: string) => f(`${baseUrl}/push/register`, { method: "POST", headers: { ...h, "Content-Type": "application/json" }, body: JSON.stringify({ token: pushToken }) } as any),
  };
}
