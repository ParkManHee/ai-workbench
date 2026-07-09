import { useEffect } from "react";
import { Stack, router } from "expo-router";
import { StatusBar } from "expo-status-bar";
import { SafeAreaProvider } from "react-native-safe-area-context";
import { KeyboardProvider } from "react-native-keyboard-controller";
import * as Notifications from "expo-notifications";
import { makeClient } from "../src/lib/api";
import type { PC } from "../src/lib/types";
import { loadPCs } from "../src/store/pcs";

async function registerPush(pc: PC): Promise<void> {
  // Push is an optional convenience (completion notifications); any failure
  // here (no FCM credentials in dev, permission denied, network error, ...)
  // must never crash or block the app.
  try {
    const perm = await Notifications.requestPermissionsAsync();
    if (!perm.granted) return;
    const expoToken = await Notifications.getExpoPushTokenAsync();
    await makeClient(pc.baseUrl, pc.token).registerPush(expoToken.data);
  } catch {
    // ignore
  }
}

export default function RootLayout() {
  useEffect(() => {
    loadPCs().then((pcs) => {
      if (pcs.length === 0) {
        router.replace("/pair");
        return;
      }
      // Best-effort: register push for the first PC only (v1 simplification).
      registerPush(pcs[0]);
    });
  }, []);

  return (
    <SafeAreaProvider>
      {/* 엣지-투-엣지에서 상태바 아이콘(시계·배터리 등)이 밝은 배경에 묻히지 않게 어두운 스타일 명시 */}
      <StatusBar style="dark" />
      <KeyboardProvider>
        <Stack>
          <Stack.Screen name="index" options={{ title: "PC" }} />
          {/* projects 타이틀은 화면에서 PC label로 동적 설정 */}
          <Stack.Screen name="projects" options={{ title: "프로젝트" }} />
          {/* sessions/[project] 타이틀은 화면에서 프로젝트명으로 동적 설정 */}
          <Stack.Screen name="sessions/[project]" options={{ title: "세션" }} />
          <Stack.Screen name="pair" options={{ title: "페어링" }} />
          {/* chat/[project] 타이틀은 화면에서 프로젝트명으로 동적 설정 */}
          <Stack.Screen name="chat/[project]" options={{ title: "실행" }} />
        </Stack>
      </KeyboardProvider>
    </SafeAreaProvider>
  );
}
