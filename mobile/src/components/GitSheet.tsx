import { useEffect, useMemo, useState } from "react";
import { ActivityIndicator, Alert, Modal, Platform, Pressable, StyleSheet, Text, TextInput, ToastAndroid, View } from "react-native";
import { makeClient } from "../lib/api";
import type { PC } from "../lib/types";
import { useTheme, type Theme } from "../lib/theme";

function toast(msg: string) {
  if (Platform.OS === "android") ToastAndroid.show(msg, ToastAndroid.LONG);
  else Alert.alert(msg);
}

// 커밋 → 푸시 → PR 을 데몬이 직접 한다. 커밋 대상은 작업트리 전체(add -A) — 무엇이 들어가는지는 diff 영역에서 먼저 본다.
export function GitSheet({ pc, project, visible, onClose, onCommitted }: {
  pc: PC;
  project: string;
  visible: boolean;
  onClose: () => void;
  onCommitted: () => void;
}) {
  const t = useTheme();
  const styles = useMemo(() => makeStyles(t), [t]);
  const [message, setMessage] = useState("");
  const [branch, setBranch] = useState("");
  const [onDefault, setOnDefault] = useState(false);
  const [busy, setBusy] = useState<string | null>(null);
  // 0 커밋 전 · 1 커밋됨 · 2 푸시됨 — 다음 단계만 누를 수 있다
  const [step, setStep] = useState(0);

  useEffect(() => {
    if (!visible) return;
    setStep(0);
    setBranch("");
    setMessage("");
    setBusy("초안 만드는 중…");
    makeClient(pc.baseUrl, pc.token).gitDraft(project)
      .then((d) => { setMessage(d.message); setOnDefault(d.on_default); })
      .catch(() => {}) // 초안 실패 → 빈 칸에서 시작
      .finally(() => setBusy(null));
  }, [visible, pc, project]);

  async function run(label: string, f: () => Promise<{ output: string }>, next: number) {
    setBusy(label);
    try {
      const r = await f();
      toast(r.output || `${label} 완료`);
      setStep(next);
      return true;
    } catch (e) {
      toast(e instanceof Error ? e.message : String(e));
      return false;
    } finally {
      setBusy(null);
    }
  }

  const c = makeClient(pc.baseUrl, pc.token);
  return (
    <Modal visible={visible} transparent animationType="fade" onRequestClose={onClose}>
      <Pressable style={styles.backdrop} onPress={onClose}>
        <Pressable style={styles.sheet} onPress={() => {}}>
          <View style={styles.header}>
            <Text style={styles.title}>커밋 · 푸시 · PR</Text>
            <Pressable onPress={onClose} hitSlop={12}>
              <Text style={styles.close}>✕</Text>
            </Pressable>
          </View>
          <TextInput style={styles.input} value={message} onChangeText={setMessage} placeholder="커밋 메시지" placeholderTextColor={t.placeholder} multiline editable={step === 0} />
          {onDefault && step === 0 ? (
            <TextInput style={styles.input} value={branch} onChangeText={setBranch} placeholder="새 브랜치 이름 (기본 브랜치에는 바로 커밋 안 함)" placeholderTextColor={t.placeholder} autoCapitalize="none" autoCorrect={false} />
          ) : null}
          {busy ? (
            <View style={styles.busy}><ActivityIndicator /><Text style={styles.busyText}>{busy}</Text></View>
          ) : (
            <View style={styles.row}>
              <Pressable style={[styles.btn, step !== 0 && styles.off]} disabled={step !== 0}
                onPress={() => run("커밋", () => c.gitCommit(project, message, branch.trim() || undefined), 1).then((ok) => { if (ok) onCommitted(); })}>
                <Text style={styles.btnText}>커밋</Text>
              </Pressable>
              <Pressable style={[styles.btn, step !== 1 && styles.off]} disabled={step !== 1} onPress={() => run("푸시", () => c.gitPush(project), 2)}>
                <Text style={styles.btnText}>푸시</Text>
              </Pressable>
              <Pressable style={[styles.btn, step !== 2 && styles.off]} disabled={step !== 2}
                onPress={() => run("PR", () => c.gitPr(project, message.split("\n")[0]), 3).then((ok) => { if (ok) onClose(); })}>
                <Text style={styles.btnText}>PR</Text>
              </Pressable>
            </View>
          )}
        </Pressable>
      </Pressable>
    </Modal>
  );
}

const makeStyles = (t: Theme) => StyleSheet.create({
  backdrop: { flex: 1, backgroundColor: "rgba(0,0,0,0.5)", justifyContent: "center", padding: 24 },
  sheet: { backgroundColor: t.box, borderRadius: 12, padding: 16, gap: 10 },
  header: { flexDirection: "row", alignItems: "center", justifyContent: "space-between" },
  title: { fontSize: 17, fontWeight: "700", color: t.text },
  close: { fontSize: 20, color: t.subtext },
  input: { borderWidth: StyleSheet.hairlineWidth, borderColor: t.border, borderRadius: 8, padding: 10, color: t.inputText, fontSize: 15 },
  row: { flexDirection: "row", gap: 8 },
  btn: { flex: 1, backgroundColor: t.accent, borderRadius: 8, paddingVertical: 12, alignItems: "center" },
  off: { opacity: 0.35 },
  btnText: { color: "#fff", fontWeight: "600" },
  busy: { flexDirection: "row", alignItems: "center", gap: 8, paddingVertical: 12 },
  busyText: { color: t.subtext },
});
