import type { ComponentProps, ReactNode } from "react";
import {
  ActivityIndicator,
  Modal,
  Pressable,
  StyleSheet,
  Text,
  View,
  type StyleProp,
  type ViewStyle,
} from "react-native";
import { MaterialIcons } from "@expo/vector-icons";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import { font, radius, space, TOUCH, useTheme } from "../lib/theme";

export type IconName = ComponentProps<typeof MaterialIcons>["name"];

export function Icon({ name, size = 20, color }: { name: IconName; size?: number; color?: string }) {
  const t = useTheme();
  // 장식 — 글리프(사설 영역 문자)가 감싼 버튼의 읽기 문구에 섞이지 않게 접근성 트리에서 뺀다. 의미는 버튼 라벨이 갖는다
  return <MaterialIcons name={name} size={size} color={color ?? t.text} accessible={false} importantForAccessibility="no" />;
}

/**
 * 한글은 음절마다 줄이 바뀔 수 있어 끝 글자 하나가 다음 줄로 떨어진다(「…이어집니 / 다」).
 * 어절 안 음절 사이에 WORD JOINER(U+2060)를 넣어 띄어쓰기에서만 줄을 바꾸게 한다 — 고정 문구용(입력값엔 쓰지 않는다)
 */
export function keepWords(s: string): string {
  return s.split(" ").map((w) => [...w].join("\u2060")).join(" ");
}

const PRESSED = 0.7;
const DISABLED = 0.4;

/** dangerOutline: 목록 줄마다 반복되는 위험 동작 — 꽉 찬 빨강이 줄줄이 서면 화면이 시끄럽다 */
type ButtonVariant = "primary" | "secondary" | "danger" | "dangerOutline" | "ghost";

export function Button({ title, onPress, variant = "primary", icon, disabled, loading, size = "md", style }: {
  title: string;
  onPress: () => void;
  variant?: ButtonVariant;
  icon?: IconName;
  disabled?: boolean;
  loading?: boolean;
  /** sm 은 줄 안에 들어가는 버튼 — 높이 40 에 hitSlop 4 로 48 을 채운다 */
  size?: "md" | "sm";
  style?: StyleProp<ViewStyle>;
}) {
  const t = useTheme();
  const bg = { primary: t.accent, secondary: t.surfaceAlt, danger: t.danger, dangerOutline: "transparent", ghost: "transparent" }[variant];
  const fg = { primary: t.onAccent, secondary: t.text, danger: t.onDanger, dangerOutline: t.danger, ghost: t.accent }[variant];
  const off = disabled || loading;
  return (
    <Pressable
      onPress={onPress}
      disabled={off}
      hitSlop={size === "sm" ? 4 : undefined}
      accessibilityRole="button"
      accessibilityLabel={title}
      accessibilityState={{ disabled: !!off, busy: !!loading }}
      style={({ pressed }) => [
        styles.button,
        { backgroundColor: bg, height: size === "sm" ? 40 : TOUCH, paddingHorizontal: size === "sm" ? space.md : space.lg },
        variant === "dangerOutline" ? { borderWidth: 1, borderColor: t.border } : null,
        disabled ? { opacity: DISABLED } : pressed ? { opacity: PRESSED } : null,
        style,
      ]}
    >
      {loading ? <ActivityIndicator size="small" color={fg} /> : icon ? <Icon name={icon} size={18} color={fg} /> : null}
      <Text style={[styles.buttonText, { color: fg, fontSize: size === "sm" ? font.size.sm : font.size.md }]} numberOfLines={1}>
        {title}
      </Text>
    </Pressable>
  );
}

/** 아이콘만 있는 버튼 — 48×48, 라벨은 스크린리더용 */
export function IconButton({ name, label, onPress, color, disabled, filled, style }: {
  name: IconName;
  label: string;
  onPress: () => void;
  color?: string;
  disabled?: boolean;
  /** accent 원형 바탕(전송 버튼 등) */
  filled?: boolean;
  style?: StyleProp<ViewStyle>;
}) {
  const t = useTheme();
  return (
    <Pressable
      onPress={onPress}
      disabled={disabled}
      accessibilityRole="button"
      accessibilityLabel={label}
      accessibilityState={{ disabled: !!disabled }}
      style={({ pressed }) => [
        styles.iconButton,
        filled ? { backgroundColor: t.accent } : null,
        disabled ? { opacity: DISABLED } : pressed ? { opacity: PRESSED } : null,
        style,
      ]}
    >
      <Icon name={name} size={24} color={color ?? (filled ? t.onAccent : t.subtext)} />
    </Pressable>
  );
}

/** 칩 — 선택지 답·프리셋·모델/effort/권한·범위 토글이 같은 모양을 쓴다. 높이 36 + hitSlop 6 = 48 */
export function Chip({ label, onPress, selected, disabled, icon, trailingIcon, children }: {
  label?: string;
  onPress: () => void;
  selected?: boolean;
  disabled?: boolean;
  icon?: IconName;
  trailingIcon?: IconName;
  /** label 대신 글자 일부만 강조할 때 */
  children?: ReactNode;
}) {
  const t = useTheme();
  const fg = selected ? t.accent : t.text;
  return (
    <Pressable
      onPress={onPress}
      disabled={disabled}
      hitSlop={6}
      accessibilityRole="button"
      accessibilityState={{ disabled: !!disabled, selected: !!selected }}
      style={({ pressed }) => [
        styles.chip,
        selected
          ? { backgroundColor: t.accentSoft, borderColor: t.accent }
          : { backgroundColor: t.surface, borderColor: t.border },
        disabled ? { opacity: DISABLED } : pressed ? { opacity: PRESSED } : null,
      ]}
    >
      {icon ? <Icon name={icon} size={16} color={fg} /> : null}
      {children ?? (
        <Text style={[styles.chipText, { color: fg }, selected ? { fontWeight: font.weight.bold } : null]} numberOfLines={1}>
          {label}
        </Text>
      )}
      {trailingIcon ? <Icon name={trailingIcon} size={16} color={t.subtext} /> : null}
    </Pressable>
  );
}

/** 목록 한 줄 — 최소 높이 56(한 줄)·72(두 줄 이상). 오른쪽은 배지·버튼·화살표 */
export function ListRow({ title, subtitle, subtitleEllipsize, right, children, onPress, onLongPress }: {
  title: string;
  subtitle?: string;
  /** 경로처럼 끝이 중요한 부제는 middle */
  subtitleEllipsize?: "middle";
  right?: ReactNode;
  /** 제목·부제 아래 추가 줄 */
  children?: ReactNode;
  onPress?: () => void;
  onLongPress?: () => void;
}) {
  const t = useTheme();
  return (
    <Pressable
      onPress={onPress}
      onLongPress={onLongPress}
      disabled={!onPress && !onLongPress}
      accessibilityRole={onPress ? "button" : undefined}
      style={({ pressed }) => [styles.row, { borderBottomColor: t.border }, pressed ? { backgroundColor: t.surface } : null]}
    >
      <View style={styles.rowBody}>
        <Text style={[styles.rowTitle, { color: t.text }]} numberOfLines={1}>{title}</Text>
        {subtitle ? <Text style={[styles.rowSubtitle, { color: t.subtext }]} numberOfLines={1} ellipsizeMode={subtitleEllipsize}>{subtitle}</Text> : null}
        {children}
      </View>
      {right}
    </Pressable>
  );
}

export type Status = "working" | "waiting";

/** 작업 중·답 필요 — PC·프로젝트·세션 목록이 모두 이 배지를 쓴다 */
export function StatusBadge({ status, label }: { status: Status; label?: string }) {
  const t = useTheme();
  const color = status === "working" ? t.success : t.warning;
  const word = status === "working" ? "작업 중" : "답 필요";
  return (
    <View style={[styles.badge, { backgroundColor: t.surfaceAlt }]}>
      <View style={[styles.dot, { backgroundColor: color }]} />
      <Text style={[styles.badgeText, { color: t.text }]} numberOfLines={1}>
        {/* 이름을 단 배지도 상태를 글자로 — 점 색만으로 구분하지 않는다 */}
        {label ? `${label} · ${word}` : word}
      </Text>
    </View>
  );
}

/** 빈 목록·오류 자리 — 가운데 아이콘·문구·선택적 버튼 */
export function EmptyState({ icon, title, action }: { icon: IconName; title: string; action?: ReactNode }) {
  const t = useTheme();
  return (
    <View style={styles.empty}>
      <Icon name={icon} size={40} color={t.subtext} />
      <Text style={[styles.emptyText, { color: t.subtext }]}>{title}</Text>
      {action}
    </View>
  );
}

/**
 * 시트 공통 틀 — X·바깥 탭·뒤로가기 모두 닫는다.
 * bottom: 고르기만 하는 시트. dialog: 입력칸이 있는 시트 — 아래에 붙이면 키보드가 가려 위쪽에 띄운다.
 */
export function SheetFrame({ visible, title, onClose, variant = "bottom", children }: {
  visible: boolean;
  title?: string;
  onClose: () => void;
  variant?: "bottom" | "dialog";
  children: ReactNode;
}) {
  const t = useTheme();
  const insets = useSafeAreaInsets();
  const bottom = variant === "bottom";
  return (
    <Modal visible={visible} transparent animationType="fade" statusBarTranslucent navigationBarTranslucent onRequestClose={onClose}>
      <Pressable
        style={[styles.backdrop, { backgroundColor: t.scrim }, bottom ? styles.backdropBottom : { paddingTop: insets.top + 72, paddingHorizontal: space.lg }]}
        onPress={onClose}
      >
        <Pressable
          style={[
            styles.sheet,
            { backgroundColor: t.elevated },
            bottom ? { borderTopLeftRadius: radius.lg, borderTopRightRadius: radius.lg, paddingBottom: insets.bottom + space.lg } : { borderRadius: radius.lg },
          ]}
          onPress={() => {}}
        >
          {bottom ? <View style={[styles.grabber, { backgroundColor: t.border }]} /> : null}
          <View style={styles.sheetHeader}>
            <Text style={[styles.sheetTitle, { color: t.text }]} numberOfLines={2}>{title}</Text>
            <IconButton name="close" label="닫기" onPress={onClose} style={{ marginRight: -space.md }} />
          </View>
          {children}
        </Pressable>
      </Pressable>
    </Modal>
  );
}

const styles = StyleSheet.create({
  button: {
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "center",
    gap: space.sm,
    borderRadius: radius.pill,
  },
  buttonText: { fontWeight: font.weight.bold },
  iconButton: {
    width: TOUCH,
    height: TOUCH,
    borderRadius: TOUCH / 2,
    alignItems: "center",
    justifyContent: "center",
  },
  chip: {
    flexDirection: "row",
    alignItems: "center",
    gap: space.xs,
    height: 36,
    paddingHorizontal: space.md,
    borderRadius: radius.sm,
    borderWidth: StyleSheet.hairlineWidth,
    maxWidth: "100%",
  },
  chipText: { fontSize: font.size.sm, flexShrink: 1 },
  row: {
    flexDirection: "row",
    alignItems: "center",
    gap: space.md,
    minHeight: 56,
    paddingHorizontal: space.lg,
    paddingVertical: space.md,
    borderBottomWidth: StyleSheet.hairlineWidth,
  },
  rowBody: { flex: 1, gap: space.xxs },
  rowTitle: { fontSize: font.size.md, fontWeight: font.weight.bold },
  rowSubtitle: { fontSize: font.size.xs },
  badge: {
    flexDirection: "row",
    alignItems: "center",
    gap: space.xs,
    paddingHorizontal: space.sm,
    height: 24,
    borderRadius: radius.pill,
    alignSelf: "flex-start",
  },
  dot: { width: 8, height: 8, borderRadius: radius.pill },
  badgeText: { fontSize: font.size.xs, fontWeight: font.weight.medium },
  empty: { flex: 1, alignItems: "center", justifyContent: "center", padding: space.xl, gap: space.md },
  emptyText: { fontSize: font.size.sm, textAlign: "center" },
  backdrop: { flex: 1 },
  backdropBottom: { justifyContent: "flex-end" },
  sheet: { paddingHorizontal: space.lg, paddingTop: space.sm, gap: space.sm },
  grabber: { alignSelf: "center", width: 32, height: 4, borderRadius: radius.pill, marginBottom: space.xs },
  sheetHeader: { flexDirection: "row", alignItems: "center", justifyContent: "space-between", gap: space.sm },
  sheetTitle: { flex: 1, fontSize: font.size.lg, fontWeight: font.weight.bold },
});
