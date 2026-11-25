import { useMemo, useState } from "react";
import { ActivityIndicator, StyleSheet, Text, View } from "react-native";
import { CameraView, useCameraPermissions, type BarcodeScanningResult } from "expo-camera";
import { router } from "expo-router";
import { parsePairPayload } from "../src/lib/pairing";
import { makeClient, pairUrl } from "../src/lib/api";
import { pcId } from "../src/lib/pcs-util";
import { addPC } from "../src/store/pcs";
import { font, media, radius, space, useTheme, type Theme } from "../src/lib/theme";
import { Button, Icon } from "../src/components/ui";

export default function Pair() {
  const t = useTheme();
  const styles = useMemo(() => makeStyles(t), [t]);
  const [permission, requestPermission] = useCameraPermissions();
  const [scanned, setScanned] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [pairing, setPairing] = useState(false);

  async function handleScan({ data }: BarcodeScanningResult) {
    if (scanned || pairing) return;
    setScanned(true);
    setError(null);

    const parsed = parsePairPayload(data);
    if (!parsed) {
      setError("페어링용 QR 코드가 아닙니다.");
      setScanned(false);
      return;
    }

    setPairing(true);
    try {
      const res = await fetch(pairUrl(parsed.baseUrl, parsed.code));
      if (!res.ok) throw new Error(`pair failed (${res.status})`);
      const body = await res.json();
      if (!body || typeof body.token !== "string") throw new Error("pair failed (no token)");
      const baseUrl = parsed.baseUrl;
      const token: string = body.token;
      let label = baseUrl.replace(/^[a-zA-Z]+:\/\//, "");
      try {
        const info = await makeClient(baseUrl, token).info();
        if (info?.hostname) label = info.hostname;
      } catch {
        // info() is best-effort; fall back to the host derived from baseUrl.
      }
      await addPC({ id: pcId(baseUrl), label, baseUrl, token });
      router.replace("/");
    } catch {
      setError("페어링하지 못했습니다. 다시 시도해 주세요.");
      setScanned(false);
    } finally {
      setPairing(false);
    }
  }

  if (!permission) {
    return <View style={styles.container} />;
  }

  if (!permission.granted) {
    return (
      <View style={[styles.container, styles.permission]}>
        <Icon name="photo-camera" size={40} color={t.subtext} />
        <Text style={styles.text}>페어링 QR 코드를 스캔하려면 카메라 권한이 필요합니다.</Text>
        <Button title="권한 허용" onPress={requestPermission} />
      </View>
    );
  }

  return (
    <View style={styles.container}>
      <CameraView
        style={StyleSheet.absoluteFill}
        facing="back"
        barcodeScannerSettings={{ barcodeTypes: ["qr"] }}
        onBarcodeScanned={scanned ? undefined : handleScan}
      />
      {/* 겨냥 틀 — 카메라 위 장식이라 누르기를 막지 않는다 */}
      <View style={styles.frame} pointerEvents="none" />
      {pairing ? (
        <View style={styles.overlay}>
          <ActivityIndicator color={media.fg} />
        </View>
      ) : error ? (
        <View style={styles.overlay}>
          <Icon name="error-outline" size={20} color={media.fg} />
          <Text style={styles.errorText}>{error}</Text>
        </View>
      ) : null}
    </View>
  );
}

// 카메라 위 요소는 테마와 무관하게 어두운 바탕·흰 글자(영상 밝기에 따라 대비가 흔들리지 않게)
const makeStyles = (t: Theme) => StyleSheet.create({
  container: {
    flex: 1,
    alignItems: "center",
    justifyContent: "center",
  },
  permission: {
    padding: space.xl,
    gap: space.lg,
  },
  text: {
    textAlign: "center",
    fontSize: font.size.md,
    color: t.text,
  },
  frame: {
    width: 240,
    height: 240,
    borderRadius: radius.lg,
    borderWidth: 3,
    borderColor: media.frame,
  },
  overlay: {
    position: "absolute",
    bottom: 40,
    left: space.lg,
    right: space.lg,
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "center",
    gap: space.sm,
    backgroundColor: media.scrim,
    borderRadius: radius.md,
    padding: space.md,
  },
  errorText: {
    color: media.fg,
    fontSize: font.size.sm,
  },
});
