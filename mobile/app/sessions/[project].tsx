import { useCallback, useEffect, useState, useMemo } from "react";
import {
  ActivityIndicator,
  Button,
  FlatList,
  Pressable,
  RefreshControl,
  StyleSheet,
  Text,
  View,
} from "react-native";
import { router, Stack, useLocalSearchParams } from "expo-router";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import { isUnauthorized, makeClient } from "../../src/lib/api";
import type { PC, SessionInfo } from "../../src/lib/types";
import { getPC, removePC } from "../../src/store/pcs";
import { useTheme, type Theme } from "../../src/lib/theme";

/** 대략적인 상대 시간(분/시간/일) 표시, 오래된 항목은 날짜로. unix seconds 입력. */
function formatUpdated(unixSeconds: number): string {
  const diffMs = Date.now() - unixSeconds * 1000;
  const diffMin = Math.floor(diffMs / 60000);
  if (diffMin < 1) return "방금 전";
  if (diffMin < 60) return `${diffMin}분 전`;
  const diffHour = Math.floor(diffMin / 60);
  if (diffHour < 24) return `${diffHour}시간 전`;
  const diffDay = Math.floor(diffHour / 24);
  if (diffDay < 7) return `${diffDay}일 전`;
  return new Date(unixSeconds * 1000).toLocaleDateString();
}

export default function Sessions() {
  const t = useTheme();
  const styles = useMemo(() => makeStyles(t), [t]);
  const { project, pc: pcId, path } = useLocalSearchParams<{
    project: string;
    pc: string;
    path: string;
  }>();
  const insets = useSafeAreaInsets();
  // undefined = not checked yet, null = checked and no PC found (redirecting)
  const [pc, setPc] = useState<PC | null | undefined>(undefined);
  const [sessions, setSessions] = useState<SessionInfo[]>([]);
  const [loading, setLoading] = useState(false);
  const [refreshing, setRefreshing] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(
    async (p: PC, isRefresh: boolean) => {
      if (isRefresh) setRefreshing(true);
      else setLoading(true);
      setError(null);
      try {
        const list = await makeClient(p.baseUrl, p.token).sessions(project);
        setSessions(list);
      } catch (e) {
        if (isUnauthorized(e)) {
          // Token revoked/invalid → drop this PC and send the user back to the PC list.
          await removePC(p.id);
          router.replace("/");
          return;
        }
        setError("Failed to load sessions. Please try again.");
      } finally {
        if (isRefresh) setRefreshing(false);
        else setLoading(false);
      }
    },
    [project],
  );

  useEffect(() => {
    let cancelled = false;
    if (!pcId) {
      setPc(null);
      router.replace("/");
      return;
    }
    getPC(pcId).then((p) => {
      if (cancelled) return;
      if (!p) {
        setPc(null);
        router.replace("/");
        return;
      }
      setPc(p);
      load(p, false);
    });
    return () => {
      cancelled = true;
    };
  }, [pcId, load]);

  function handleRetry() {
    if (pc) load(pc, false);
  }

  function handleRefresh() {
    if (pc) load(pc, true);
  }

  function handleNewChat() {
    router.push({
      pathname: "/chat/[project]",
      params: { pc: pcId, project, path },
    });
  }

  function handlePress(item: SessionInfo) {
    router.push({
      pathname: "/chat/[project]",
      params: { pc: pcId, project, path, session: item.session_id },
    });
  }

  // PC not resolved yet, or not found (redirect in flight): render nothing.
  if (!pc) {
    return <View style={styles.container} />;
  }

  return (
    <View style={styles.container}>
      <Stack.Screen options={{ title: project }} />
      <View style={styles.header}>
        <Pressable style={styles.newButton} onPress={handleNewChat}>
          <Text style={styles.newButtonText}>+ 새 대화</Text>
        </Pressable>
      </View>
      {loading ? (
        <View style={styles.center}>
          <ActivityIndicator />
        </View>
      ) : error ? (
        <View style={styles.center}>
          <Text style={styles.errorText}>{error}</Text>
          <Button title="Retry" onPress={handleRetry} />
        </View>
      ) : (
        <FlatList
          data={sessions}
          keyExtractor={(item) => item.session_id}
          contentContainerStyle={{ paddingBottom: insets.bottom + 12, flexGrow: 1 }}
          refreshControl={<RefreshControl refreshing={refreshing} onRefresh={handleRefresh} />}
          ListEmptyComponent={
            <View style={styles.center}>
              <Text>No sessions found.</Text>
            </View>
          }
          renderItem={({ item }) => (
            <Pressable style={styles.row} onPress={() => handlePress(item)}>
              <View style={styles.rowTop}>
                <Text style={styles.preview} numberOfLines={1}>
                  {item.preview || "(빈 대화)"}
                </Text>
                {item.active ? <Text style={styles.activeBadge}>🟢</Text> : item.waiting ? <Text style={styles.activeBadge}>🔴</Text> : null}
              </View>
              <Text style={styles.updated}>{formatUpdated(item.updated)}</Text>
            </Pressable>
          )}
        />
      )}
    </View>
  );
}

const makeStyles = (t: Theme) => StyleSheet.create({
  container: {
    flex: 1,
  },
  center: {
    flex: 1,
    alignItems: "center",
    justifyContent: "center",
    padding: 24,
    gap: 12,
  },
  header: {
    padding: 12,
    alignItems: "flex-end",
  },
  newButton: {
    backgroundColor: "#2f6fed",
    borderRadius: 8,
    paddingHorizontal: 14,
    paddingVertical: 8,
  },
  newButtonText: {
    color: "white",
    fontWeight: "600",
  },
  errorText: {
    textAlign: "center",
    color: t.text,
  },
  row: {
    padding: 16,
    borderBottomWidth: StyleSheet.hairlineWidth,
    borderBottomColor: t.border,
  },
  rowTop: {
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "space-between",
    gap: 8,
  },
  preview: {
    fontSize: 16,
    fontWeight: "600",
    flex: 1,
    color: t.text,
  },
  activeBadge: {
    fontSize: 12,
  },
  updated: {
    fontSize: 12,
    color: t.subtext,
    marginTop: 2,
  },
});
