import { useEffect, useRef } from "react";
import { Stack, router, usePathname } from "expo-router";
import { StatusBar } from "expo-status-bar";
import { SafeAreaProvider } from "react-native-safe-area-context";
import { KeyboardProvider } from "react-native-keyboard-controller";
import * as Notifications from "expo-notifications";
import { makeClient } from "../src/lib/api";
import { PROGRESS_CHANNEL } from "../src/lib/progress";
import type { PC } from "../src/lib/types";
import { loadPCs } from "../src/store/pcs";
import { font, useTheme } from "../src/lib/theme";

// 앱이 포그라운드일 때도 알림을 표시(기본은 억제됨)
Notifications.setNotificationHandler({
  handleNotification: async () => ({
    shouldShowBanner: true,
    shouldShowList: true,
    shouldPlaySound: true,
    shouldSetBadge: false,
  }),
});

async function registerPush(pcs: PC[]): Promise<void> {
  // Push is an optional convenience (completion notifications); any failure
  // here (no FCM credentials in dev, permission denied, network error, ...)
  // must never crash or block the app.
  try {
    // Android 8+ 필수: 채널이 없으면 알림이 조용히 버려진다
    await Notifications.setNotificationChannelAsync("default", {
      name: "실행 알림",
      importance: Notifications.AndroidImportance.HIGH,
    });
    // 진행 알림은 15초마다 덮어써지므로 소리·진동·헤드업 없이
    await Notifications.setNotificationChannelAsync(PROGRESS_CHANNEL, {
      name: "진행 중",
      importance: Notifications.AndroidImportance.LOW,
      sound: null,
      enableVibrate: false,
    });
    const perm = await Notifications.requestPermissionsAsync();
    if (!perm.granted) {
      console.warn("[push] permission not granted:", perm.status);
      return;
    }
    const expoToken = await Notifications.getExpoPushTokenAsync();
    // PC 마다 따로 catch — 꺼진 PC 하나가 다른 PC 등록을 막지 않게
    await Promise.all(pcs.map((pc) =>
      makeClient(pc.baseUrl, pc.token).registerPush(expoToken.data).catch((e) => console.warn("[push] register failed:", pc.baseUrl, e))
    ));
  } catch (e) {
    console.warn("[push] register failed:", e);
  }
}

/** 알림 탭 → 해당 PC의 프로젝트 대화방으로 딥링크. 데몬이 data에 hostname/project/path/session을 담아 보낸다. */
async function openFromNotification(data: Record<string, unknown> | undefined): Promise<void> {
  const project = typeof data?.project === "string" ? data.project : null;
  if (!project) return;
  const pcs = await loadPCs();
  // 페어링 시 label을 데몬 hostname으로 저장하므로 hostname으로 PC를 찾는다(없으면 첫 PC)
  const pc = pcs.find((p) => p.label === data?.hostname) ?? pcs[0];
  if (!pc) return;
  router.push({
    pathname: "/chat/[project]",
    params: {
      project,
      pc: pc.id,
      path: typeof data?.path === "string" ? data.path : "",
      ...(typeof data?.session === "string" && data.session ? { session: data.session } : {}),
    },
  });
}

export default function RootLayout() {
  const pathname = usePathname();
  const pushTried = useRef(new Set<string>());
  // 첫 실행엔 PC 가 없어 등록을 건너뛰므로, 화면이 바뀔 때마다 아직 시도 안 한 PC(새로 페어링한 PC 포함)를 등록한다
  useEffect(() => {
    loadPCs().then((pcs) => {
      const fresh = pcs.filter((p) => !pushTried.current.has(p.id));
      if (fresh.length === 0) return;
      fresh.forEach((p) => pushTried.current.add(p.id));
      registerPush(fresh);
    });
  }, [pathname]);

  useEffect(() => {
    loadPCs().then((pcs) => {
      if (pcs.length === 0) router.replace("/pair");
    });
    // 앱이 떠 있거나 백그라운드일 때 알림 탭
    const sub = Notifications.addNotificationResponseReceivedListener((resp) => {
      openFromNotification(resp.notification.request.content.data as Record<string, unknown>);
    });
    // 알림 탭으로 콜드 스타트한 경우
    Notifications.getLastNotificationResponseAsync().then((resp) => {
      if (resp) openFromNotification(resp.notification.request.content.data as Record<string, unknown>);
    });
    return () => sub.remove();
  }, []);

  const t = useTheme();
  return (
    <SafeAreaProvider>
      {/* 상태바 아이콘: 시스템 테마에 맞춰 자동(라이트→어두운 아이콘, 다크→밝은 아이콘) */}
      <StatusBar style="auto" />
      <KeyboardProvider>
        <Stack
          screenOptions={{
            headerStyle: { backgroundColor: t.bg },
            headerTintColor: t.text,
            headerTitleStyle: { fontSize: font.size.lg, fontWeight: font.weight.bold },
            headerShadowVisible: false,
            contentStyle: { backgroundColor: t.bg },
          }}
        >
          <Stack.Screen name="index" options={{ title: "PC" }} />
          {/* projects 타이틀은 화면에서 PC label로 동적 설정 */}
          <Stack.Screen name="projects" options={{ title: "프로젝트" }} />
          {/* sessions/[project] 타이틀은 화면에서 프로젝트명으로 동적 설정 */}
          <Stack.Screen name="sessions/[project]" options={{ title: "세션" }} />
          <Stack.Screen name="pair" options={{ title: "페어링" }} />
          <Stack.Screen name="devices" options={{ title: "페어링 기기" }} />
          <Stack.Screen name="needs" options={{ title: "답 필요" }} />
          {/* chat/[project] 타이틀은 화면에서 프로젝트명으로 동적 설정 */}
          <Stack.Screen name="chat/[project]" options={{ title: "실행" }} />
        </Stack>
      </KeyboardProvider>
    </SafeAreaProvider>
  );
}
