import { describe, it, expect } from "vitest";
import { parseManualPair, parsePairPayload } from "./pairing";
describe("parseManualPair", () => {
  it("주소:포트와 6자 코드 → 스캔과 같은 모양, 소문자 코드는 대문자로", () => {
    expect(parseManualPair(" 100.119.50.46:8787 ", "ab2c3d"))
      .toEqual({ baseUrl: "http://100.119.50.46:8787", code: "AB2C3D" });
  });
  it("http:// 접두어·끝 슬래시도 받는다", () => {
    expect(parseManualPair("http://my-mac.local:8787/", "ABC234"))
      .toEqual({ baseUrl: "http://my-mac.local:8787", code: "ABC234" });
  });
  it("awb:// 전체를 붙이면 코드칸과 무관하게 그 안의 코드", () => {
    expect(parseManualPair("awb://100.64.0.1:8787?code=XYZ789", ""))
      .toEqual({ baseUrl: "http://100.64.0.1:8787", code: "XYZ789" });
  });
  it("포트 없음·범위 밖·https·빈 값은 주소 오류", () => {
    for (const a of ["100.64.0.1", "100.64.0.1:0", "100.64.0.1:70000", "https://h:1", "", "h:80/x"]) {
      expect(parseManualPair(a, "ABC234")).toEqual({ error: "addr" });
    }
  });
  it("6자가 아니거나 기호가 섞이면 코드 오류", () => {
    for (const c of ["ABC23", "ABC2345", "ABC-23", ""]) {
      expect(parseManualPair("h:8787", c)).toEqual({ error: "code" });
    }
  });
});
describe("parsePairPayload", () => {
  it("valid awb URL", () => {
    expect(parsePairPayload("awb://100.64.0.1:8787?code=ABC234"))
      .toEqual({ baseUrl: "http://100.64.0.1:8787", code: "ABC234" });
  });
  it("rejects non-awb", () => { expect(parsePairPayload("https://x?code=1")).toBeNull(); });
  it("rejects missing code", () => { expect(parsePairPayload("awb://1.2.3.4:80")).toBeNull(); });
});
