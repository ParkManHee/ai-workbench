import { describe, it, expect } from "vitest";
import { pairUrl, streamUrl, makeClient, HttpError, isUnauthorized } from "./api";
describe("url builders", () => {
  it("streamUrl uses ws scheme + token + offset", () => {
    expect(streamUrl("http://1.2.3.4:8787", "r1", 0, "tok"))
      .toBe("ws://1.2.3.4:8787/stream/r1?offset=0&token=tok");
  });
  it("pairUrl", () => { expect(pairUrl("http://1.2.3.4:8787","AB")).toBe("http://1.2.3.4:8787/pair?code=AB"); });
  it("client.chat posts to /chat/:project", async () => {
    const calls: any[] = [];
    const fetchMock = async (url: string, init: any) => { calls.push([url, init]); return { ok: true, json: async () => ({ run_id: "r1", log: "l" }) }; };
    const c = makeClient("http://1.2.3.4:8787", "tok", fetchMock as any);
    const r = await c.chat("demo", "hi", true);
    expect(r.run_id).toBe("r1");
    expect(calls[0][0]).toBe("http://1.2.3.4:8787/chat/demo");
    expect(JSON.parse(calls[0][1].body)).toEqual({ prompt: "hi", plan: true });
    expect(calls[0][1].headers.Authorization).toBe("Bearer tok");
  });
});
describe("401 handling", () => {
  const notOk = (status: number) =>
    (async () => ({ ok: false, status, json: async () => ({}) })) as any;
  it("client.chat rejects with HttpError(401) on unauthorized", async () => {
    const c = makeClient("http://1.2.3.4:8787", "tok", notOk(401));
    await expect(c.chat("demo", "hi", false)).rejects.toBeInstanceOf(HttpError);
    await c.chat("demo", "hi", false).catch((e) => expect(e.status).toBe(401));
  });
  it("client.projects rejects with HttpError(401)", async () => {
    const c = makeClient("http://1.2.3.4:8787", "tok", notOk(401));
    await c.projects().catch((e) => { expect(e).toBeInstanceOf(HttpError); expect(e.status).toBe(401); });
  });
  it("isUnauthorized is true only for HttpError 401", () => {
    expect(isUnauthorized(new HttpError(401, "/x"))).toBe(true);
    expect(isUnauthorized(new HttpError(500, "/x"))).toBe(false);
    expect(isUnauthorized(new Error("network"))).toBe(false);
    expect(isUnauthorized(null)).toBe(false);
  });
});
