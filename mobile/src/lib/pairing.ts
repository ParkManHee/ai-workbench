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

/** 손으로 적은 PC 주소·코드 → 스캔과 같은 {baseUrl, code}. 틀린 칸을 돌려줘 그 칸 아래에 원인을 보인다 */
export function parseManualPair(addr: string, code: string):
  { baseUrl: string; code: string } | { error: "addr" | "code" } {
  const a = addr.trim();
  // QR 내용(awb://host:port?code=…)을 통째로 붙여 넣은 경우 — 그 안의 코드를 쓴다
  const payload = parsePairPayload(a);
  if (payload) return payload;
  const m = /^(?:http:\/\/)?([A-Za-z0-9.-]+):(\d{1,5})\/?$/i.exec(a);
  if (!m || Number(m[2]) < 1 || Number(m[2]) > 65535) return { error: "addr" };
  const c = code.trim().toUpperCase();
  if (!/^[A-Z0-9]{6}$/.test(c)) return { error: "code" };
  return { baseUrl: `http://${m[1]}:${m[2]}`, code: c };
}
