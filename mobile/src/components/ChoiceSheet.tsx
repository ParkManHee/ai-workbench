import { useMemo } from "react";
import { Pressable, ScrollView, StyleSheet, Text, View } from "react-native";
import { font, space, useTheme, type Theme } from "../lib/theme";
import { Button, Icon, SheetFrame } from "./ui";

export interface Choice {
  label: string;
  onPress: () => void;
  /** 지금 고른 값 — 체크로 보인다 */
  selected?: boolean;
  /** 라벨만으론 차이를 모르는 선택지의 한 줄 설명 */
  description?: string;
  /** 되돌릴 수 없는 동작 — 빨간 글자 */
  destructive?: boolean;
}
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
            accessibilityState={{ selected: !!c.selected }}
            onPress={() => {
              onClose();
              c.onPress();
            }}
          >
            <View style={styles.choiceBody}>
              <Text style={[styles.choiceText, c.selected ? styles.choiceSelected : null, c.destructive ? { color: t.danger } : null]}>{c.label}</Text>
              {c.description ? <Text style={styles.choiceDesc}>{c.description}</Text> : null}
            </View>
            {c.selected ? <Icon name="check" size={20} color={t.accent} /> : null}
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
    flexDirection: "row",
    alignItems: "center",
    gap: space.sm,
    paddingVertical: space.sm,
    paddingHorizontal: space.xs,
    borderBottomWidth: StyleSheet.hairlineWidth,
    borderBottomColor: t.border,
  },
  choiceBody: { flex: 1, gap: space.xxs },
  choiceText: { fontSize: font.size.md, color: t.text },
  choiceSelected: { color: t.accent, fontWeight: font.weight.bold },
  choiceDesc: { fontSize: font.size.xs, color: t.subtext },
  cancel: { marginTop: space.sm },
});
