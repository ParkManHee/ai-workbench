import { useEffect } from "react";
import { Stack, router } from "expo-router";
import { loadSession } from "../src/store/session";

export default function RootLayout() {
  useEffect(() => {
    loadSession().then((session) => {
      if (!session) router.replace("/pair");
    });
  }, []);

  return <Stack />;
}
