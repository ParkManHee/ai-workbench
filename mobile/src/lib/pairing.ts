export function parsePairPayload(s: string): { baseUrl: string; code: string } | null {
  if (!s.startsWith("awb://")) return null;
  const rest = s.slice("awb://".length);
  const [hostPort, query] = rest.split("?");
  if (!hostPort || !query) return null;
  const params = new URLSearchParams(query);
  const code = params.get("code");
  if (!code) return null;
  return { baseUrl: `http://${hostPort}`, code };
}
