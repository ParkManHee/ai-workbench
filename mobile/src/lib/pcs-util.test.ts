import { describe, it, expect } from "vitest";
import { upsertPC, pcId } from "./pcs-util";
import type { PC } from "./types";

const mk = (baseUrl: string, label = "Mac"): PC => ({ id: pcId(baseUrl), label, baseUrl, token: "tok" });

describe("pcId", () => {
  it("derives a stable id from baseUrl", () => {
    expect(pcId("http://1.2.3.4:8787")).toBe(pcId("http://1.2.3.4:8787"));
  });
  it("differs for different baseUrls", () => {
    expect(pcId("http://1.2.3.4:8787")).not.toBe(pcId("http://1.2.3.5:8787"));
  });
  it("is a sanitized host:port (no scheme, no special chars)", () => {
    expect(pcId("http://1.2.3.4:8787")).toBe("1-2-3-4-8787");
  });
});

describe("upsertPC", () => {
  it("appends a new PC when baseUrl is not already present", () => {
    const list: PC[] = [mk("http://1.2.3.4:8787", "A")];
    const next = upsertPC(list, mk("http://5.6.7.8:8787", "B"));
    expect(next).toHaveLength(2);
    expect(next.map((p) => p.baseUrl)).toEqual(["http://1.2.3.4:8787", "http://5.6.7.8:8787"]);
  });
  it("replaces the existing entry with the same baseUrl (no duplicates)", () => {
    const list: PC[] = [mk("http://1.2.3.4:8787", "Old Name")];
    const next = upsertPC(list, mk("http://1.2.3.4:8787", "New Name"));
    expect(next).toHaveLength(1);
    expect(next[0].label).toBe("New Name");
  });
  it("does not mutate the input list", () => {
    const list: PC[] = [mk("http://1.2.3.4:8787", "A")];
    upsertPC(list, mk("http://5.6.7.8:8787", "B"));
    expect(list).toHaveLength(1);
  });
});
