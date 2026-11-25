import { useMemo } from "react";
import { Pressable, ScrollView, StyleSheet, Text } from "react-native";
import { font, space, useTheme, type Theme } from "../lib/theme";
import { Button, SheetFrame } from "./ui";

export interface Choice { label: string; onPress: () => void }
export interface ChoiceSheetState { title: string; message?: string; choices: Choice[] }

// Alert.alert 대신 쓰는 선택 시트 — Android Alert 는 버튼을 3개까지만 그려 4번째(취소)가 잘리고
// 뒤로가기로도 닫히지 않아 빠져나올 수 없었다(2026-09-23 폰 실측). X·취소·바깥 탭·뒤로가기 모두 닫는다.
export function ChoiceSheet({ state, onClose }: { state: ChoiceSheetState | null; onClose: () => void }) {
  const t = useTheme();
  const styles = useMemo(() => makeStyles(t), [t]);
  return (
    <SheetFrame visible={!!state} title={state?.title} onClose={onClose}>
      {state?.message ? <Text style={styles.message}>{state.message}</Text> : null}
      <ScrollView style={{ maxHeight: 360 }}>
        {state?.choices.map((c) => (
          <Pressable
            key={c.label}
            style={({ pressed }) => [styles.choice, pressed ? { backgroundColor: t.surfaceAlt } : null]}
            accessibilityRole="button"
            onPress={() => {
              onClose();
              c.onPress();
            }}
          >
            <Text style={styles.choiceText}>{c.label}</Text>
          </Pressable>
        ))}
      </ScrollView>
      <Button title="취소" variant="secondary" onPress={onClose} style={styles.cancel} />
    </SheetFrame>
  );
}

const makeStyles = (t: Theme) => StyleSheet.create({
  message: { fontSize: font.size.sm, color: t.subtext },
  choice: {
    minHeight: 52,
    justifyContent: "center",
    paddingHorizontal: space.xs,
    borderBottomWidth: StyleSheet.hairlineWidth,
    borderBottomColor: t.border,
  },
  choiceText: { fontSize: font.size.md, color: t.text },
  cancel: { marginTop: space.sm },
});
