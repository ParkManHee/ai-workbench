import { useCallback, useState } from "react";
import { Alert, Button, FlatList, Pressable, StyleSheet, Text, View } from "react-native";
import { router, useFocusEffect } from "expo-router";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import { makeClient } from "../src/lib/api";
import type { PC, Project } from "../src/lib/types";
import { loadPCs, removePC } from "../src/store/pcs";

function hostOf(baseUrl: string): string {
  return baseUrl.replace(/^[a-zA-Z]+:\/\//, "");
}

/** PC 한 대의 활동 요약 한 줄: "🟢 proj1 · 🔴 proj2" (상태 있는 프로젝트만). */
function statusLine(projects: Project[]): string {
  const parts = projects
    .filter((p) => p.agent_status)
    .map((p) => `${p.agent_status === "working" ? "🟢" : "🔴"} ${p.name}`);
  return parts.join(" · ");
}

export default function Index() {
  const insets = useSafeAreaInsets();
  // undefined = not checked yet, null = checked and empty (redirecting)
  const [pcs, setPcs] = useState<PC[] | null | undefined>(undefined);
  // PC별 활동 요약(대시보드): pcId → 요약 문자열("" = 활동 없음, undefined = 조회 전/실패)
  const [statusById, setStatusById] = useState<Record<string, string>>({});

  const refresh = useCallback(async () => {
    const list = await loadPCs();
    if (list.length === 0) {
      setPcs(null);
      router.replace("/pair");
      return;
    }
    setPcs(list);
    // 각 PC의 프로젝트 상태를 병렬 best-effort 조회(오프라인 PC는 조용히 스킵)
    list.forEach(async (pc) => {
      try {
        const projects: Project[] = await makeClient(pc.baseUrl, pc.token).projects();
        setStatusById((prev) => ({ ...prev, [pc.id]: statusLine(projects) }));
      } catch {
        setStatusById((prev) => {
          const next = { ...prev };
          delete next[pc.id];
          return next;
        });
      }
    });
  }, []);

  // Re-check whenever this screen regains focus (e.g. back from /pair after
  // adding a PC, or after deleting one) so the list is always current.
  useFocusEffect(
    useCallback(() => {
      refresh();
    }, [refresh])
  );

  function handlePress(pc: PC) {
    router.push({ pathname: "/projects", params: { pc: pc.id } });
  }

  function handleLongPress(pc: PC) {
    Alert.alert("PC 삭제", `"${pc.label}"을(를) 목록에서 삭제할까요?`, [
      { text: "취소", style: "cancel" },
      {
        text: "삭제",
        style: "destructive",
        onPress: async () => {
          await removePC(pc.id);
          refresh();
        },
      },
    ]);
  }

  if (!pcs) {
    return <View style={styles.container} />;
  }

  return (
    <View style={styles.container}>
      <View style={styles.header}>
        <Button title="+ PC 추가" onPress={() => router.push("/pair")} />
      </View>
      <FlatList
        data={pcs}
        keyExtractor={(item) => item.id}
        contentContainerStyle={{ paddingBottom: insets.bottom + 12, flexGrow: 1 }}
        ListEmptyComponent={
          <View style={styles.center}>
            <Text>등록된 PC가 없습니다.</Text>
          </View>
        }
        renderItem={({ item }) => (
          <Pressable
            style={styles.row}
            onPress={() => handlePress(item)}
            onLongPress={() => handleLongPress(item)}
          >
            <Text style={styles.label}>{item.label}</Text>
            <Text style={styles.host}>{hostOf(item.baseUrl)}</Text>
            {statusById[item.id] ? (
              <Text style={styles.statusLine}>{statusById[item.id]}</Text>
            ) : statusById[item.id] === "" ? (
              <Text style={styles.statusIdle}>활동 중인 프로젝트 없음</Text>
            ) : null}
          </Pressable>
        )}
      />
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
  },
  header: {
    padding: 12,
    alignItems: "flex-start",
  },
  center: {
    flex: 1,
    alignItems: "center",
    justifyContent: "center",
    padding: 24,
    gap: 12,
  },
  row: {
    padding: 16,
    borderBottomWidth: StyleSheet.hairlineWidth,
    borderBottomColor: "#ccc",
  },
  label: {
    fontSize: 16,
    fontWeight: "600",
  },
  host: {
    fontSize: 12,
    color: "#666",
    marginTop: 2,
  },
  statusLine: {
    fontSize: 13,
    marginTop: 6,
  },
  statusIdle: {
    fontSize: 12,
    color: "#999",
    marginTop: 6,
  },
});
