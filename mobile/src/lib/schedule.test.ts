import { describe, expect, it } from "vitest";
import { nextLabel } from "./schedule";

const at = (y: number, m: number, d: number, h: number, min: number) => new Date(y, m - 1, d, h, min).getTime() / 1000;
// 2026-11-25 수요일 18:00
const now = new Date(2026, 10, 25, 18, 0);

describe("nextLabel", () => {
  it("오늘 아직 안 온 시각은 「오늘」", () => {
    expect(nextLabel(at(2026, 11, 25, 21, 5), now)).toBe("오늘 21:05");
  });
  it("다음 날 회차는 「내일」", () => {
    expect(nextLabel(at(2026, 11, 26, 9, 0), now)).toBe("내일 09:00");
  });
  it("오늘 같은 요일 시각이 지났으면 7일 뒤 회차 — 날짜를 붙인다", () => {
    expect(nextLabel(at(2026, 12, 2, 17, 23), now)).toBe("12/2(수) 17:23");
  });
});
