import { useMemo } from "react";
import { Modal, Pressable, ScrollView, StyleSheet, Text, View } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import { useTheme, type Theme } from "../lib/theme";

// 말풍선 일부만 복사 — 말풍선의 Markdown 렌더는 드래그 선택이 안 돼서 원문을 selectable Text 로 따로 띄운다.
export function TextSelectModal({ text, onClose, onCopyAll }: {
  text: string | null;
  onClose: () => void;
  onCopyAll: (text: string) => void;
}) {
  const t = useTheme();
  const styles = useMemo(() => makeStyles(t), [t]);
  const insets = useSafeAreaInsets();
  return (
    <Modal visible={text !== null} animationType="slide" onRequestClose={onClose}>
      <View style={[styles.root, { paddingTop: insets.top, paddingBottom: insets.bottom }]}>
        <View style={styles.header}>
          <Text style={styles.hint}>원하는 부분을 길게 눌러 선택하세요</Text>
          <Pressable onPress={onClose} hitSlop={12}>
            <Text style={styles.close}>✕</Text>
          </Pressable>
        </View>
        <ScrollView style={styles.body} contentContainerStyle={styles.bodyContent}>
          <Text style={styles.text} selectable>
            {text ?? ""}
          </Text>
        </ScrollView>
        <Pressable
          style={styles.copyAll}
          onPress={() => {
            if (text !== null) onCopyAll(text);
            onClose();
          }}
        >
          <Text style={styles.copyAllText}>전체 복사</Text>
        </Pressable>
      </View>
    </Modal>
  );
}

const makeStyles = (t: Theme) => StyleSheet.create({
  root: { flex: 1, backgroundColor: t.box },
  header: {
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "space-between",
    paddingHorizontal: 16,
    paddingVertical: 12,
    borderBottomWidth: StyleSheet.hairlineWidth,
    borderBottomColor: t.border,
  },
  hint: { flex: 1, color: t.subtext },
  close: { fontSize: 20, color: t.subtext },
  body: { flex: 1 },
  bodyContent: { padding: 16 },
  text: { fontSize: 16, lineHeight: 24, color: t.text },
  copyAll: {
    paddingVertical: 14,
    alignItems: "center",
    borderTopWidth: StyleSheet.hairlineWidth,
    borderTopColor: t.border,
  },
  copyAllText: { fontSize: 16, color: t.accent, fontWeight: "600" },
});
