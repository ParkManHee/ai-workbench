import { useCallback, useMemo, useState } from "react";
import { Alert, FlatList, Pressable, StyleSheet, Text, View } from "react-native";
import { router, Stack, useFocusEffect, useLocalSearchParams } from "expo-router";
import { isUnauthorized, makeClient } from "../src/lib/api";
import type { PC } from "../src/lib/types";
import { getPC, removePC } from "../src/store/pcs";
import { useTheme, type Theme } from "../src/lib/theme";

interface DeviceDto { id: string; label: string; paired_at: number }

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
      {error ? <Text style={styles.error}>{error}</Text> : null}
      <FlatList
        data={devices}
        keyExtractor={(d) => d.id}
        ListEmptyComponent={<Text style={styles.empty}>페어링된 기기가 없습니다.</Text>}
        renderItem={({ item }) => (
          <View style={styles.row}>
            <View style={styles.info}>
              <Text style={styles.label}>{item.label}</Text>
              <Text style={styles.meta}>
                {item.id} · {new Date(item.paired_at * 1000).toLocaleDateString("ko-KR")} 페어링
              </Text>
            </View>
            <Pressable style={styles.revoke} onPress={() => confirmRevoke(item)}>
              <Text style={styles.revokeText}>회수</Text>
            </Pressable>
          </View>
        )}
      />
    </View>
  );
}

const makeStyles = (t: Theme) => StyleSheet.create({
  container: { flex: 1 },
  row: {
    flexDirection: "row",
    alignItems: "center",
    padding: 16,
    borderBottomWidth: StyleSheet.hairlineWidth,
    borderBottomColor: t.border,
  },
  info: { flex: 1 },
  label: { fontSize: 16, fontWeight: "600", color: t.text },
  meta: { fontSize: 12, color: t.subtext, marginTop: 2 },
  revoke: {
    backgroundColor: "#c0392b",
    borderRadius: 8,
    paddingHorizontal: 14,
    paddingVertical: 8,
  },
  revokeText: { color: "white", fontWeight: "600" },
  empty: { textAlign: "center", marginTop: 40, color: t.subtext },
  error: { color: "#c0392b", textAlign: "center", paddingVertical: 6 },
});
