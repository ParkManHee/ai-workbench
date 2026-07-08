import { useCallback, useEffect, useState } from "react";
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
import { router } from "expo-router";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import { isUnauthorized, makeClient } from "../src/lib/api";
import type { Preflight, Project } from "../src/lib/types";
import { clearSession, loadSession, type Session } from "../src/store/session";

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

export default function Index() {
  const insets = useSafeAreaInsets();
  // undefined = not checked yet, null = checked and no session (redirecting)
  const [session, setSession] = useState<Session | null | undefined>(undefined);
  const [projects, setProjects] = useState<Project[]>([]);
  const [preflight, setPreflight] = useState<Preflight | null>(null);
  const [loading, setLoading] = useState(false);
  const [refreshing, setRefreshing] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async (s: Session, isRefresh: boolean) => {
    if (isRefresh) setRefreshing(true);
    else setLoading(true);
    setError(null);
    try {
      const client = makeClient(s.baseUrl, s.token);
      const [projectList, preflightResult] = await Promise.all([
        client.projects(),
        client.preflight(),
      ]);
      setProjects(projectList);
      setPreflight(preflightResult);
    } catch (e) {
      if (isUnauthorized(e)) {
        // Token revoked/invalid → drop it and send the user back to pairing.
        await clearSession();
        router.replace("/pair");
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
    loadSession().then((s) => {
      if (cancelled) return;
      if (!s) {
        setSession(null);
        router.replace("/pair");
        return;
      }
      setSession(s);
      load(s, false);
    });
    return () => {
      cancelled = true;
    };
  }, [load]);

  // Session not checked yet, or no session (redirect in flight): render nothing.
  if (!session) {
    return <View style={styles.container} />;
  }

  function handleRetry() {
    if (session) load(session, false);
  }

  function handleRefresh() {
    if (session) load(session, true);
  }

  function handlePress(project: Project) {
    router.push({
      pathname: "/chat/[project]",
      params: { project: project.name, path: project.path },
    });
  }

  return (
    <View style={styles.container}>
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
                <Text style={styles.name}>{item.name}</Text>
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
