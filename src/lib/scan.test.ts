import { describe, it, expect } from "vitest";
import { filterProjects } from "./scan";
describe("filterProjects", () => {
  it("이름 부분일치·대소문자 무시", () => {
    const list = [{name:"csms-api"},{name:"Java-OCA"}] as any;
    expect(filterProjects(list,"oca").map(p=>p.name)).toEqual(["Java-OCA"]);
    expect(filterProjects(list,"").length).toBe(2);
  });
});
