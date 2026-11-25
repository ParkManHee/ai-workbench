import { useMemo } from "react";
import { Modal, ScrollView, StyleSheet, Text, View } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import { font, space, useTheme, type Theme } from "../lib/theme";
import { Button, IconButton } from "./ui";

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
    <Modal visible={text !== null} animationType="slide" statusBarTranslucent navigationBarTranslucent onRequestClose={onClose}>
      <View style={[styles.root, { paddingTop: insets.top, paddingBottom: insets.bottom + space.md }]}>
        <View style={styles.header}>
          <Text style={styles.hint}>원하는 부분을 길게 눌러 선택하세요</Text>
          <IconButton name="close" label="닫기" onPress={onClose} />
        </View>
        <ScrollView style={styles.body} contentContainerStyle={styles.bodyContent}>
          <Text style={styles.text} selectable>
            {text ?? ""}
          </Text>
        </ScrollView>
        <Button
          title="전체 복사"
          icon="content-copy"
          style={styles.copyAll}
          onPress={() => {
            if (text !== null) onCopyAll(text);
            onClose();
          }}
        />
      </View>
    </Modal>
  );
}

const makeStyles = (t: Theme) => StyleSheet.create({
  root: { flex: 1, backgroundColor: t.bg },
  header: {
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "space-between",
    paddingLeft: space.lg,
    paddingRight: space.xs,
    borderBottomWidth: StyleSheet.hairlineWidth,
    borderBottomColor: t.border,
  },
  hint: { flex: 1, fontSize: font.size.sm, color: t.subtext },
  body: { flex: 1 },
  bodyContent: { padding: space.lg },
  text: { fontSize: font.size.md, lineHeight: 24, color: t.text },
  copyAll: { marginHorizontal: space.lg },
});
