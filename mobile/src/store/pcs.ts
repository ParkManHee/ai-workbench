import * as SecureStore from "expo-secure-store";
import type { PC } from "../lib/types";
import { upsertPC } from "../lib/pcs-util";

const K_PCS = "awb_pcs";

export async function loadPCs(): Promise<PC[]> {
  const raw = await SecureStore.getItemAsync(K_PCS);
  if (!raw) return [];
  try {
    const parsed = JSON.parse(raw);
    return Array.isArray(parsed) ? parsed : [];
  } catch {
    return [];
  }
}

export async function savePCs(list: PC[]): Promise<void> {
  await SecureStore.setItemAsync(K_PCS, JSON.stringify(list));
}

export async function addPC(pc: PC): Promise<PC[]> {
  const list = await loadPCs();
  const next = upsertPC(list, pc);
  await savePCs(next);
  return next;
}

export async function removePC(id: string): Promise<PC[]> {
  const list = await loadPCs();
  const next = list.filter((p) => p.id !== id);
  await savePCs(next);
  return next;
}

export async function getPC(id: string): Promise<PC | null> {
  const list = await loadPCs();
  return list.find((p) => p.id === id) ?? null;
}
