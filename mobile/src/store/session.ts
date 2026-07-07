import * as SecureStore from "expo-secure-store";

const K_BASE = "awb_base_url";
const K_TOK = "awb_token";

export interface Session {
  baseUrl: string;
  token: string;
}

export async function saveSession(baseUrl: string, token: string): Promise<void> {
  await SecureStore.setItemAsync(K_BASE, baseUrl);
  await SecureStore.setItemAsync(K_TOK, token);
}

export async function loadSession(): Promise<Session | null> {
  const baseUrl = await SecureStore.getItemAsync(K_BASE);
  const token = await SecureStore.getItemAsync(K_TOK);
  return baseUrl && token ? { baseUrl, token } : null;
}

export async function clearSession(): Promise<void> {
  await SecureStore.deleteItemAsync(K_BASE);
  await SecureStore.deleteItemAsync(K_TOK);
}
