import { useCallback, useState } from "react";
import { Alert, Button, FlatList, Pressable, StyleSheet, Text, View } from "react-native";
import { router, useFocusEffect } from "expo-router";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import type { PC } from "../src/lib/types";
import { loadPCs, removePC } from "../src/store/pcs";

function hostOf(baseUrl: string): string {
  return baseUrl.replace(/^[a-zA-Z]+:\/\//, "");
}

export default function Index() {
  const insets = useSafeAreaInsets();
  // undefined = not checked yet, null = checked and empty (redirecting)
  const [pcs, setPcs] = useState<PC[] | null | undefined>(undefined);

  const refresh = useCallback(async () => {
    const list = await loadPCs();
    if (list.length === 0) {
      setPcs(null);
      router.replace("/pair");
      return;
    }
    setPcs(list);
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
});
