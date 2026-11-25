import { useEffect, useMemo, useState } from "react";
import { ActivityIndicator, Alert, Platform, StyleSheet, Text, TextInput, ToastAndroid, View } from "react-native";
import { makeClient } from "../lib/api";
import type { PC } from "../lib/types";
import { font, radius, space, TOUCH, useTheme, type Theme } from "../lib/theme";
import { Button, SheetFrame } from "./ui";

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
    <SheetFrame visible={visible} title="커밋 · 푸시 · PR" variant="dialog" onClose={onClose}>
      <TextInput style={[styles.input, styles.message]} textBreakStrategy="balanced" value={message} onChangeText={setMessage} placeholder="커밋 메시지" placeholderTextColor={t.placeholder} multiline editable={step === 0} />
      {onDefault && step === 0 ? (
        <TextInput style={[styles.input, styles.branch]} value={branch} onChangeText={setBranch} placeholder="새 브랜치 이름 (기본 브랜치에는 바로 커밋 안 함)" placeholderTextColor={t.placeholder} autoCapitalize="none" autoCorrect={false} />
      ) : null}
      {busy ? (
        <View style={styles.busy}><ActivityIndicator color={t.accent} /><Text style={styles.busyText}>{busy}</Text></View>
      ) : (
        <View style={styles.row}>
          <Button title="커밋" icon="check" style={styles.btn} disabled={step !== 0}
            onPress={() => run("커밋", () => c.gitCommit(project, message, branch.trim() || undefined), 1).then((ok) => { if (ok) onCommitted(); })} />
          <Button title="푸시" icon="upload" style={styles.btn} disabled={step !== 1} onPress={() => run("푸시", () => c.gitPush(project), 2)} />
          <Button title="PR" icon="call-merge" style={styles.btn} disabled={step !== 2}
            onPress={() => run("PR", () => c.gitPr(project, message.split("\n")[0]), 3).then((ok) => { if (ok) onClose(); })} />
        </View>
      )}
    </SheetFrame>
  );
}

const makeStyles = (t: Theme) => StyleSheet.create({
  input: {
    minHeight: TOUCH,
    paddingHorizontal: space.md,
    paddingVertical: space.md,
    borderRadius: radius.md,
    borderWidth: 1,
    borderColor: t.inputBorder,
    color: t.text,
    fontSize: font.size.md,
  },
  message: { maxHeight: 160, textAlignVertical: "top" },
  // 안내 문구가 길어 16 이면 두 줄로 넘어가 잘린다
  branch: { height: TOUCH, paddingVertical: 0, fontSize: font.size.sm },
  row: { flexDirection: "row", gap: space.sm, paddingTop: space.xs, paddingBottom: space.lg },
  btn: { flex: 1, paddingHorizontal: space.sm },
  busy: { flexDirection: "row", alignItems: "center", gap: space.sm, height: TOUCH, marginBottom: space.lg },
  busyText: { fontSize: font.size.sm, color: t.subtext },
});
