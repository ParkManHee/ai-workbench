import { useMemo, useState } from "react";
import { ActivityIndicator, StyleSheet, Text, TextInput, View } from "react-native";
import { CameraView, useCameraPermissions, type BarcodeScanningResult } from "expo-camera";
import { router } from "expo-router";
import { parseManualPair, parsePairPayload } from "../src/lib/pairing";
import { makeClient, pairUrl } from "../src/lib/api";
import { pcId } from "../src/lib/pcs-util";
import { addPC } from "../src/store/pcs";
import { font, media, radius, space, TOUCH, useTheme, type Theme } from "../src/lib/theme";
import { Button, Icon, SheetFrame } from "../src/components/ui";

/** 스캔·손입력이 함께 타는 페어링 — 실패하면 원인 문구, 성공하면 null. again: 그 경로에서 다시 하는 법 */
async function pairWithPc(parsed: { baseUrl: string; code: string }, again: string): Promise<string | null> {
  try {
    let res: Response;
    try {
      res = await fetch(pairUrl(parsed.baseUrl, parsed.code));
    } catch {
      return "PC에 연결하지 못했습니다. PC가 켜져 있고 같은 네트워크(Tailscale)에 있는지 확인해 주세요.";
    }
    // 데몬은 만료·이미 쓴 코드·틀린 코드를 모두 403 으로 돌려준다 — 셋을 구별할 수 없어 한 문구로
    if (res.status === 403) {
      return `코드가 만료됐거나 이미 사용됐습니다. PC에서 새 페어링 QR을 띄운 뒤 ${again}.`;
    }
    const body = res.ok ? await res.json().catch(() => null) : null;
    if (!body || typeof body.token !== "string") {
      return `페어링하지 못했습니다(응답 ${res.status}). ${again}.`;
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
    return null;
  } catch {
    return `페어링하지 못했습니다. ${again}.`;
  }
}

export default function Pair() {
  const t = useTheme();
  const styles = useMemo(() => makeStyles(t), [t]);
  const [permission, requestPermission] = useCameraPermissions();
  const [scanned, setScanned] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [pairing, setPairing] = useState(false);
  const [manualOpen, setManualOpen] = useState(false);
  const [addr, setAddr] = useState("");
  const [code, setCode] = useState("");
  const [fieldError, setFieldError] = useState<"addr" | "code" | null>(null);
  const [manualError, setManualError] = useState<string | null>(null);

  // PC 목록에서 왔으면 그 화면으로 돌아가고(replace 는 목록을 한 겹 더 쌓는다), 첫 실행이라 목록이 없으면 이 화면을 목록으로 바꾼다
  async function runPair(parsed: { baseUrl: string; code: string }, again: string): Promise<string | null> {
    setPairing(true);
    const err = await pairWithPc(parsed, again);
    setPairing(false);
    if (!err) router.dismissTo("/");
    return err;
  }

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
    setError(await runPair(parsed, "다시 스캔해 주세요"));
  }

  function changeAddr(v: string) {
    setAddr(v);
    setFieldError(null);
    // QR 내용을 통째로 붙여 넣으면 코드칸을 채운다
    const p = parsePairPayload(v.trim());
    if (p) setCode(p.code);
  }

  async function submitManual() {
    if (pairing) return;
    setManualError(null);
    const parsed = parseManualPair(addr, code);
    if ("error" in parsed) {
      setFieldError(parsed.error);
      return;
    }
    setManualError(await runPair(parsed, "다시 시도해 주세요"));
  }

  function openManual() {
    setManualError(null);
    setFieldError(null);
    setManualOpen(true);
  }

  const manualButton = <Button title="주소·코드로 추가" icon="keyboard" variant="secondary" onPress={openManual} />;
  const manualSheet = (
    <SheetFrame visible={manualOpen} title="주소·코드로 추가" variant="dialog" onClose={() => setManualOpen(false)}>
      <Text style={styles.label}>PC 주소</Text>
      <TextInput
        style={[styles.input, fieldError === "addr" ? { borderColor: t.danger } : null]}
        value={addr}
        onChangeText={changeAddr}
        placeholder="100.119.50.46:8787"
        placeholderTextColor={t.placeholder}
        accessibilityLabel="PC 주소"
        autoCapitalize="none"
        autoCorrect={false}
        keyboardType="url"
        editable={!pairing}
      />
      {fieldError === "addr" ? <Text style={styles.fieldError}>주소:포트 모양으로 적어 주세요(예: 100.119.50.46:8787).</Text> : null}
      <Text style={styles.label}>코드(6자리)</Text>
      <TextInput
        style={[styles.input, fieldError === "code" ? { borderColor: t.danger } : null]}
        value={code}
        onChangeText={(v) => { setCode(v.toUpperCase()); setFieldError(null); }}
        placeholder="PC에 표시된 6자리"
        placeholderTextColor={t.placeholder}
        accessibilityLabel="코드 6자리"
        autoCapitalize="characters"
        autoCorrect={false}
        maxLength={6}
        editable={!pairing}
        returnKeyType="go"
        onSubmitEditing={submitManual}
      />
      {fieldError === "code" ? <Text style={styles.fieldError}>코드는 영문 대문자·숫자 6자입니다.</Text> : null}
      {manualError ? (
        <View style={styles.manualError} accessibilityLiveRegion="polite">
          <Icon name="error-outline" size={20} color={t.danger} />
          <Text style={styles.manualErrorText}>{manualError}</Text>
        </View>
      ) : null}
      <Button title="페어링" icon="link" loading={pairing} onPress={submitManual} style={styles.submit} />
    </SheetFrame>
  );

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
        {manualButton}
        {manualSheet}
      </View>
    );
  }

  return (
    <View style={styles.container}>
      <CameraView
        style={StyleSheet.absoluteFill}
        facing="back"
        barcodeScannerSettings={{ barcodeTypes: ["qr"] }}
        onBarcodeScanned={scanned || manualOpen ? undefined : handleScan}
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
          {manualButton}
        </View>
      ) : (
        <View style={styles.overlay}>
          <Text style={styles.overlayText}>PC에 띄운 페어링 QR 코드를 틀 안에 비춰 주세요</Text>
          {manualButton}
        </View>
      )}
      {manualSheet}
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
  label: { fontSize: font.size.sm, color: t.subtext, fontWeight: font.weight.medium },
  input: {
    minHeight: TOUCH,
    paddingHorizontal: space.md,
    borderRadius: radius.md,
    borderWidth: 1,
    borderColor: t.inputBorder,
    fontSize: font.size.md,
    color: t.text,
  },
  fieldError: { fontSize: font.size.xs, color: t.danger },
  manualError: { flexDirection: "row", alignItems: "flex-start", gap: space.sm },
  manualErrorText: { flex: 1, fontSize: font.size.sm, color: t.text },
  submit: { marginTop: space.xs, marginBottom: space.lg },
  overlayText: {
    flexShrink: 1,
    color: media.fg,
    fontSize: font.size.sm,
    textAlign: "center",
  },
});
