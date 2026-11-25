import { useCallback, useMemo, useState } from "react";
import { Alert, FlatList, StyleSheet, Text, View } from "react-native";
import { router, Stack, useFocusEffect, useLocalSearchParams } from "expo-router";
import { isUnauthorized, makeClient } from "../src/lib/api";
import type { PC } from "../src/lib/types";
import { getPC, removePC } from "../src/store/pcs";
import { font, space, useTheme, type Theme } from "../src/lib/theme";
import { Button, EmptyState, Icon, ListRow } from "../src/components/ui";

interface DeviceDto { id: string; label: string; paired_at: number }

/** 데몬이 붙이는 라벨은 모두 "mobile" 이라 이름만으론 못 가린다 — id 앞 4자를 붙이고 부제에 페어링 시각(분까지)을 둔다 */
function pairedAt(sec: number): string {
  const d = new Date(sec * 1000);
  return `${d.getFullYear()}. ${d.getMonth() + 1}. ${d.getDate()}. ${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
}

/** 페어링 기기 관리 — 분실 폰 등의 토큰을 회수(즉시 401)한다. */
export default function Devices() {
  const t = useTheme();
  const styles = useMemo(() => makeStyles(t), [t]);
  const { pc: pcId } = useLocalSearchParams<{ pc: string }>();
  const [pc, setPc] = useState<PC | null>(null);
  const [devices, setDevices] = useState<DeviceDto[]>([]);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    const p = pcId ? await getPC(pcId) : null;
    if (!p) { router.back(); return; }
    setPc(p);
    try {
      setDevices(await makeClient(p.baseUrl, p.token).devices());
      setError(null);
    } catch (e) {
      if (isUnauthorized(e)) { await removePC(p.id); router.replace("/"); return; }
      setError("기기 목록 조회 실패");
    }
  }, [pcId]);

  useFocusEffect(useCallback(() => { load(); }, [load]));

  function confirmRevoke(d: DeviceDto) {
    Alert.alert(
      "기기 회수",
      `"${d.label}" (${d.id})의 접근 토큰을 회수할까요?\n그 기기는 즉시 연결이 끊기고 재페어링해야 합니다.\n(이 폰의 토큰을 회수하면 본인도 로그아웃됩니다)`,
      [
        { text: "취소", style: "cancel" },
        {
          text: "회수", style: "destructive",
          onPress: async () => {
            if (!pc) return;
            try {
              await makeClient(pc.baseUrl, pc.token).revokeDevice(d.id);
              load();
            } catch (e) {
              if (isUnauthorized(e)) { await removePC(pc.id); router.replace("/"); return; }
              setError("회수 실패");
            }
          },
        },
      ]
    );
  }

  return (
    <View style={styles.container}>
      <Stack.Screen options={{ title: `페어링 기기 — ${pc?.label ?? ""}` }} />
      {error ? (
        <View style={styles.error}>
          <Icon name="error-outline" size={16} color={t.danger} />
          <Text style={styles.errorText}>{error}</Text>
        </View>
      ) : null}
      <FlatList
        data={devices}
        keyExtractor={(d) => d.id}
        contentContainerStyle={{ flexGrow: 1 }}
        ListEmptyComponent={<EmptyState icon="devices" title="페어링된 기기가 없습니다." />}
        renderItem={({ item }) => (
          <ListRow
            title={`${item.label} #${item.id.slice(0, 4)}${item.id === pc?.deviceId ? " · 이 기기" : ""}`}
            subtitle={`${pairedAt(item.paired_at)} 페어링 · ${item.id}`}
            right={<Button title="회수" variant="dangerOutline" size="sm" onPress={() => confirmRevoke(item)} />}
          />
        )}
      />
    </View>
  );
}

const makeStyles = (t: Theme) => StyleSheet.create({
  container: { flex: 1 },
  error: {
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "center",
    gap: space.xs,
    paddingVertical: space.sm,
  },
  errorText: { color: t.danger, fontSize: font.size.sm },
});
