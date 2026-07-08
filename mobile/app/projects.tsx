import { useCallback, useEffect, useRef, useState } from "react";
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
import { router, Stack, useFocusEffect, useLocalSearchParams } from "expo-router";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import { isUnauthorized, makeClient } from "../src/lib/api";
import type { PC, Preflight, Project } from "../src/lib/types";
import { getPC, removePC } from "../src/store/pcs";

function badgeText(project: Project): string | null {
  const b = project.badge;
  if (!b) return null;
  return `⬜${b.todo} 🔄${b.doing} ✅${b.done}`;
}

function preflightText(preflight: Preflight | null): string {
  if (!preflight) return "";
  if (preflight.claude_path) return "✅ Claude ready";
  const failing = preflight.checks.find((c) => !c.ok);
  return failing ? `⚠️ Not ready: ${failing.detail}` : "⚠️ Claude path not resolved";
}

export default function Projects() {
  const { pc: pcId } = useLocalSearchParams<{ pc: string }>();
  const insets = useSafeAreaInsets();
  // undefined = not checked yet, null = checked and no PC found (redirecting)
  const [pc, setPc] = useState<PC | null | undefined>(undefined);
  const [projects, setProjects] = useState<Project[]>([]);
  const [preflight, setPreflight] = useState<Preflight | null>(null);
  const [loading, setLoading] = useState(false);
  const [refreshing, setRefreshing] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async (p: PC, isRefresh: boolean) => {
    if (isRefresh) setRefreshing(true);
    else setLoading(true);
    setError(null);
    try {
      const client = makeClient(p.baseUrl, p.token);
      const [projectList, preflightResult] = await Promise.all([
        client.projects(),
        client.preflight(),
      ]);
      setProjects(projectList);
      setPreflight(preflightResult);
    } catch (e) {
      if (isUnauthorized(e)) {
        // Token revoked/invalid → drop this PC and send the user back to the PC list.
        await removePC(p.id);
        router.replace("/");
        return;
      }
      setError("Failed to load projects. Please try again.");
    } finally {
      if (isRefresh) setRefreshing(false);
      else setLoading(false);
    }
  }, []);

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
    });
    return () => {
      cancelled = true;
    };
  }, [pcId, load]);

  // 화면에 돌아올 때마다 재조회(첫 로드는 전체 로딩, 이후엔 당김새로고침 스피너만).
  const hasLoadedRef = useRef(false);
  useFocusEffect(
    useCallback(() => {
      if (!pc) return;
      load(pc, hasLoadedRef.current);
      hasLoadedRef.current = true;
    }, [pc, load])
  );

  function handleRetry() {
    if (pc) load(pc, false);
  }

  function handleRefresh() {
    if (pc) load(pc, true);
  }

  function handlePress(project: Project) {
    router.push({
      pathname: "/sessions/[project]",
      params: { pc: pcId, project: project.name, path: project.path },
    });
  }

  // PC not resolved yet, or not found (redirect in flight): render nothing.
  if (!pc) {
    return <View style={styles.container} />;
  }

  return (
    <View style={styles.container}>
      <Stack.Screen options={{ title: pc.label }} />
      {preflight ? (
        <View style={styles.preflightBanner}>
          <Text style={styles.preflightText}>{preflightText(preflight)}</Text>
        </View>
      ) : null}
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
          data={projects}
          keyExtractor={(item) => item.name}
          contentContainerStyle={{ paddingBottom: insets.bottom + 12, flexGrow: 1 }}
          refreshControl={<RefreshControl refreshing={refreshing} onRefresh={handleRefresh} />}
          ListEmptyComponent={
            <View style={styles.center}>
              <Text>No projects found.</Text>
            </View>
          }
          renderItem={({ item }) => {
            const badge = badgeText(item);
            return (
              <Pressable style={styles.row} onPress={() => handlePress(item)}>
                <Text style={styles.name}>
                  {item.agent_status === "working" ? "🟢 " : item.agent_status === "waiting" ? "🔴 " : ""}
                  {item.name}
                </Text>
                <Text style={styles.path}>{item.path}</Text>
                {badge ? <Text style={styles.badge}>{badge}</Text> : null}
              </Pressable>
            );
          }}
        />
      )}
    </View>
  );
}

const styles = StyleSheet.create({
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
  preflightBanner: {
    padding: 8,
    backgroundColor: "#f0f0f0",
  },
  preflightText: {
    textAlign: "center",
  },
  errorText: {
    textAlign: "center",
  },
  row: {
    padding: 16,
    borderBottomWidth: StyleSheet.hairlineWidth,
    borderBottomColor: "#ccc",
  },
  name: {
    fontSize: 16,
    fontWeight: "600",
  },
  path: {
    fontSize: 12,
    color: "#666",
    marginTop: 2,
  },
  badge: {
    marginTop: 4,
  },
});
