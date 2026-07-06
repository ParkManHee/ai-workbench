export type { Project } from "./types";
export function filterProjects<T extends {name:string}>(list: T[], q: string): T[] {
  const s = q.trim().toLowerCase();
  return s ? list.filter(p => p.name.toLowerCase().includes(s)) : list;
}
