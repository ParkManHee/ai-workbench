import { useEffect } from "react";
import { Stack, router } from "expo-router";
import * as Notifications from "expo-notifications";
import { makeClient } from "../src/lib/api";
import { loadSession, type Session } from "../src/store/session";

async function registerPush(session: Session): Promise<void> {
  // Push is an optional convenience (completion notifications); any failure
  // here (no FCM credentials in dev, permission denied, network error, ...)
  // must never crash or block the app.
  try {
    const perm = await Notifications.requestPermissionsAsync();
    if (!perm.granted) return;
    const expoToken = await Notifications.getExpoPushTokenAsync();
    await makeClient(session.baseUrl, session.token).registerPush(expoToken.data);
  } catch {
    // ignore
  }
}

export default function RootLayout() {
  useEffect(() => {
    loadSession().then((session) => {
      if (!session) {
        router.replace("/pair");
        return;
      }
      registerPush(session);
    });
  }, []);

  return <Stack />;
}
