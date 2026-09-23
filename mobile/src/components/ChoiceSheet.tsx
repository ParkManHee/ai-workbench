import { useMemo } from "react";
import { Modal, Pressable, ScrollView, StyleSheet, Text, View } from "react-native";
import { useTheme, type Theme } from "../lib/theme";

export interface Choice { label: string; onPress: () => void }
export interface ChoiceSheetState { title: string; message?: string; choices: Choice[] }

// Alert.alert 대신 쓰는 선택 시트 — Android Alert 는 버튼을 3개까지만 그려 4번째(취소)가 잘리고
// 뒤로가기로도 닫히지 않아 빠져나올 수 없었다(2026-09-23 폰 실측). X·취소·바깥 탭·뒤로가기 모두 닫는다.
export function ChoiceSheet({ state, onClose }: { state: ChoiceSheetState | null; onClose: () => void }) {
  const t = useTheme();
  const styles = useMemo(() => makeStyles(t), [t]);
  return (
    <Modal visible={!!state} transparent animationType="fade" onRequestClose={onClose}>
      <Pressable style={styles.backdrop} onPress={onClose}>
        <Pressable style={styles.sheet} onPress={() => {}}>
          <View style={styles.header}>
            <Text style={styles.title}>{state?.title}</Text>
            <Pressable onPress={onClose} hitSlop={12}>
              <Text style={styles.close}>✕</Text>
            </Pressable>
          </View>
          {state?.message ? <Text style={styles.message}>{state.message}</Text> : null}
          <ScrollView style={{ maxHeight: 360 }}>
            {state?.choices.map((c) => (
              <Pressable
                key={c.label}
                style={styles.choice}
                onPress={() => {
                  onClose();
                  c.onPress();
                }}
              >
                <Text style={styles.choiceText}>{c.label}</Text>
              </Pressable>
            ))}
          </ScrollView>
          <Pressable style={styles.cancel} onPress={onClose}>
            <Text style={styles.cancelText}>취소</Text>
          </Pressable>
        </Pressable>
      </Pressable>
    </Modal>
  );
}

const makeStyles = (t: Theme) => StyleSheet.create({
  backdrop: { flex: 1, backgroundColor: "rgba(0,0,0,0.5)", justifyContent: "center", padding: 24 },
  sheet: { backgroundColor: t.box, borderRadius: 12, padding: 16, gap: 8 },
  header: { flexDirection: "row", alignItems: "center", justifyContent: "space-between" },
  title: { fontSize: 17, fontWeight: "700", color: t.text },
  close: { fontSize: 20, color: t.subtext },
  message: { color: t.subtext },
  choice: { paddingVertical: 12, borderBottomWidth: StyleSheet.hairlineWidth, borderBottomColor: t.border },
  choiceText: { fontSize: 16, color: t.text },
  cancel: { paddingVertical: 12, alignItems: "center" },
  cancelText: { fontSize: 16, color: t.accent, fontWeight: "600" },
});
