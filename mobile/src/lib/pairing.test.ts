import { describe, it, expect } from "vitest";
import { parsePairPayload } from "./pairing";
describe("parsePairPayload", () => {
  it("valid awb URL", () => {
    expect(parsePairPayload("awb://100.64.0.1:8787?code=ABC234"))
      .toEqual({ baseUrl: "http://100.64.0.1:8787", code: "ABC234" });
  });
  it("rejects non-awb", () => { expect(parsePairPayload("https://x?code=1")).toBeNull(); });
  it("rejects missing code", () => { expect(parsePairPayload("awb://1.2.3.4:80")).toBeNull(); });
});
