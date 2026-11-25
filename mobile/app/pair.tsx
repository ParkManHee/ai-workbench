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

  // 실패하면 카메라 인식을 멈춘 채(scanned 유지) 원인과 「다시 스캔」 을 보인다 — 바로 재인식하면 같은 QR 로 같은 실패가 반복된다
  async function handleScan({ data }: BarcodeScanningResult) {
    if (scanned || pairing) return;
    setScanned(true);
    setError(null);

    const parsed = parsePairPayload(data);
    if (!parsed) {
      setError("페어링용 QR 코드가 아닙니다. PC에 띄운 페어링 QR을 비춰 주세요.");
      return;
    }

    setPairing(true);
    try {
      let res: Response;
      try {
        res = await fetch(pairUrl(parsed.baseUrl, parsed.code));
      } catch {
        setError("PC에 연결하지 못했습니다. PC가 켜져 있고 같은 네트워크(Tailscale)에 있는지 확인해 주세요.");
        return;
      }
      // 데몬은 만료·이미 쓴 코드·틀린 코드를 모두 403 으로 돌려준다 — 셋을 구별할 수 없어 한 문구로
      if (res.status === 403) {
        setError("코드가 만료됐거나 이미 사용됐습니다. PC에서 새 페어링 QR을 띄운 뒤 다시 스캔해 주세요.");
        return;
      }
      const body = res.ok ? await res.json().catch(() => null) : null;
      if (!body || typeof body.token !== "string") {
        setError(`페어링하지 못했습니다(응답 ${res.status}). 다시 스캔해 주세요.`);
        return;
      }
      const baseUrl = parsed.baseUrl;
      const token: string = body.token;
      let label = baseUrl.replace(/^[a-zA-Z]+:\/\//, "");
      try {
        const info = await makeClient(baseUrl, token).info();
        if (info?.hostname) label = info.hostname;
      } catch {
        // info() is best-effort; fall back to the host derived from baseUrl.
      }
      const deviceId = typeof body.device_id === "string" ? body.device_id : undefined;
      await addPC({ id: pcId(baseUrl), label, baseUrl, token, ...(deviceId ? { deviceId } : {}) });
      // PC 목록에서 왔으면 그 화면으로 돌아가고(replace 는 목록을 한 겹 더 쌓는다), 첫 실행이라 목록이 없으면 이 화면을 목록으로 바꾼다
      router.dismissTo("/");
    } catch {
      setError("페어링하지 못했습니다. 다시 스캔해 주세요.");
    } finally {
      setPairing(false);
    }
  }

  function rescan() {
    setError(null);
    setScanned(false);
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
        <View style={styles.overlay} accessibilityLiveRegion="polite">
          <View style={styles.overlayRow}>
            <ActivityIndicator color={media.fg} />
            <Text style={styles.overlayText}>페어링 중…</Text>
          </View>
        </View>
      ) : error ? (
        <View style={styles.overlay} accessibilityLiveRegion="polite">
          <View style={styles.overlayRow}>
            <Icon name="error-outline" size={20} color={media.fg} />
            <Text style={styles.overlayText}>{error}</Text>
          </View>
          <Button title="다시 스캔" icon="qr-code-scanner" onPress={rescan} />
        </View>
      ) : (
        <View style={styles.overlay} pointerEvents="none">
          <Text style={styles.overlayText}>PC에 띄운 페어링 QR 코드를 틀 안에 비춰 주세요</Text>
        </View>
      )}
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
    alignItems: "center",
    gap: space.md,
    backgroundColor: media.scrim,
    borderRadius: radius.md,
    padding: space.md,
  },
  overlayRow: {
    flexDirection: "row",
    alignItems: "center",
    gap: space.sm,
  },
  overlayText: {
    flexShrink: 1,
    color: media.fg,
    fontSize: font.size.sm,
    textAlign: "center",
  },
});
