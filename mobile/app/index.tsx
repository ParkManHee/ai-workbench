import { useCallback, useState, useMemo } from "react";
import { FlatList, Pressable, RefreshControl, StyleSheet, Text, View } from "react-native";
import { router, Stack, useFocusEffect } from "expo-router";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import { EXPECTED_API_VERSION, makeClient } from "../src/lib/api";
import type { PC, Project } from "../src/lib/types";
import { loadPCs, removePC, touchPCs } from "../src/store/pcs";
import { presenceLabel } from "../src/lib/pcs-util";
import { font, radius, space, TOUCH, useTheme, type Theme } from "../src/lib/theme";
import { EmptyState, Icon, ListRow, StatusBadge } from "../src/components/ui";
import { ChoiceSheet, type ChoiceSheetState } from "../src/components/ChoiceSheet";
import { fetchNeeds, needsCount, NEEDS_POLL_MS } from "../src/lib/needs";

function hostOf(baseUrl: string): string {
  return baseUrl.replace(/^[a-zA-Z]+:\/\//, "");
}

/** PC 한 대의 활동 요약 — 상태 있는 프로젝트만. 배지로 그리려고 문자열 대신 목록으로 둔다 */
function activeProjects(projects: Project[]): Project[] {
  return projects.filter((p) => p.agent_status);
}

/** /health 는 인증 필요 라우트라 토큰을 싣는다. 401 이어도 응답이 왔으면 PC 는 켜져 있다. */
async function pingHealth(pc: PC): Promise<boolean> {
  const ctl = new AbortController();
  const timer = setTimeout(() => ctl.abort(), 3000);
  try {
    await fetch(`${pc.baseUrl}/health`, { headers: { Authorization: `Bearer ${pc.token}` }, signal: ctl.signal });
    return true;
  } catch {
    return false;
  } finally {
    clearTimeout(timer);
  }
}

export default function Index() {
  const t = useTheme();
  const styles = useMemo(() => makeStyles(t), [t]);
  const insets = useSafeAreaInsets();
  // undefined = not checked yet, null = checked and empty (redirecting)
  const [pcs, setPcs] = useState<PC[] | null | undefined>(undefined);
  // PC별 활동 요약(대시보드): pcId → 상태 있는 프로젝트([] = 활동 없음, undefined = 조회 전/실패)
  const [statusById, setStatusById] = useState<Record<string, Project[]>>({});
  // PC별 버전 불일치 경고(api_version ≠ 앱 기대값)
  const [versionWarnById, setVersionWarnById] = useState<Record<string, string>>({});
  // PC별 온라인 여부(undefined = 확인 전)
  const [onlineById, setOnlineById] = useState<Record<string, boolean>>({});
  const [refreshing, setRefreshing] = useState(false);
  const [sheet, setSheet] = useState<ChoiceSheetState | null>(null);
  // 모든 PC 의 권한 요청 + 답 대기 세션 수(undefined = 아직 모름)
  const [needs, setNeeds] = useState<number | undefined>(undefined);

  const refreshNeeds = useCallback(async () => {
    const list = await loadPCs();
    setNeeds(needsCount(await fetchNeeds(list)));
  }, []);

  const refresh = useCallback(async () => {
    const list = await loadPCs();
    if (list.length === 0) {
      setPcs(null);
      router.replace("/pair");
      return;
    }
    setPcs(list);
    Promise.all(list.map(async (pc) => {
      const ok = await pingHealth(pc);
      setOnlineById((prev) => ({ ...prev, [pc.id]: ok }));
      return ok ? pc.id : null;
    })).then(async (ids) => {
      const now = Date.now();
      const seen: Record<string, number> = {};
      for (const id of ids) if (id) seen[id] = now;
      if (Object.keys(seen).length === 0) return;
      await touchPCs(seen);
      setPcs((cur) => cur && cur.map((p) => (seen[p.id] !== undefined ? { ...p, lastSeen: now } : p)));
    });
    // 각 PC의 프로젝트 상태를 병렬 best-effort 조회(오프라인 PC는 조용히 스킵)
    list.forEach(async (pc) => {
      try {
        const client = makeClient(pc.baseUrl, pc.token);
        const [projects, info] = await Promise.all([client.projects() as Promise<Project[]>, client.info()]);
        setStatusById((prev) => ({ ...prev, [pc.id]: activeProjects(projects) }));
        setVersionWarnById((prev) => {
          const next = { ...prev };
          if (info.api_version !== undefined && info.api_version !== EXPECTED_API_VERSION) {
            next[pc.id] = info.api_version > EXPECTED_API_VERSION
              ? "⚠️ 앱 업데이트 필요 (데몬이 더 최신)"
              : "⚠️ 데몬 업데이트 필요 (앱이 더 최신)";
          } else {
            delete next[pc.id];
          }
          return next;
        });
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
      refreshNeeds();
      const iv = setInterval(refreshNeeds, NEEDS_POLL_MS);
      return () => clearInterval(iv);
    }, [refresh, refreshNeeds])
  );

  async function handleRefresh() {
    setRefreshing(true);
    await Promise.all([refresh(), refreshNeeds()]);
    setRefreshing(false);
  }

  function handlePress(pc: PC) {
    router.push({ pathname: "/projects", params: { pc: pc.id } });
  }

  function handleLongPress(pc: PC) {
    setSheet({ title: pc.label, message: "무엇을 할까요?", choices: [
      { label: "페어링 기기 관리", onPress: () => router.push({ pathname: "/devices", params: { pc: pc.id } }) },
      {
        label: "목록에서 삭제",
        destructive: true,
        onPress: async () => {
          await removePC(pc.id);
          refresh();
        },
      },
    ] });
  }

  if (!pcs) {
    return <View style={styles.container} />;
  }

  return (
    <View style={styles.container}>
      <Stack.Screen
        options={{
          headerRight: () => (
            <View style={styles.headerActions}>
              {/* 0 이어도 숨기지 않고 흐리게 — 응답 없는 PC 는 수에서 빠지므로 목록에서 그걸 확인할 수 있어야 하고, 숨기면 헤더가 들썩인다 */}
              <Pressable
                style={({ pressed }) => [styles.headerAction, !needs ? { opacity: 0.5 } : pressed ? { opacity: 0.7 } : null]}
                onPress={() => router.push("/needs")}
                accessibilityRole="button"
                accessibilityLabel={needs === undefined ? "답 필요 목록" : `답 필요 ${needs}건`}
              >
                <Icon name="notifications" size={20} color={needs ? t.warning : t.subtext} />
                <Text style={[styles.headerActionText, { color: needs ? t.warning : t.subtext }]}>
                  {needs === undefined ? "답 필요" : `답 필요 ${needs}`}
                </Text>
              </Pressable>
              <Pressable
                style={({ pressed }) => [styles.headerAction, pressed ? { opacity: 0.7 } : null]}
                onPress={() => router.push("/pair")}
                accessibilityRole="button"
              >
                <Icon name="add" size={20} color={t.accent} />
                <Text style={styles.headerActionText}>PC 추가</Text>
              </Pressable>
            </View>
          ),
        }}
      />
      <FlatList
        data={pcs}
        refreshControl={<RefreshControl refreshing={refreshing} onRefresh={handleRefresh} colors={[t.accent]} progressBackgroundColor={t.surface} />}
        keyExtractor={(item) => item.id}
        contentContainerStyle={{ paddingBottom: insets.bottom + space.md, flexGrow: 1 }}
        ListEmptyComponent={<EmptyState icon="computer" title="등록된 PC가 없습니다." />}
        renderItem={({ item }) => {
          const online = onlineById[item.id];
          const active = statusById[item.id];
          return (
            <ListRow
              title={item.label}
              subtitle={hostOf(item.baseUrl)}
              onPress={() => handlePress(item)}
              onLongPress={() => handleLongPress(item)}
              right={<Icon name="chevron-right" size={24} color={t.subtext} />}
            >
              <View style={styles.presence}>
                <View style={[styles.dot, { backgroundColor: online === undefined ? t.border : online ? t.success : t.subtext }]} />
                <Text style={styles.presenceText}>
                  {presenceLabel(online, item.lastSeen, Date.now()).replace(/^(🟢|⚫) /, "")}
                </Text>
              </View>
              {active && active.length > 0 ? (
                <View style={styles.badges}>
                  {active.map((p) => (
                    <StatusBadge key={p.name} status={p.agent_status!} label={p.name} />
                  ))}
                </View>
              ) : active ? (
                <Text style={styles.meta}>활동 중인 프로젝트 없음</Text>
              ) : null}
              {versionWarnById[item.id] ? (
                <View style={styles.presence}>
                  <Icon name="warning-amber" size={16} color={t.warning} />
                  <Text style={styles.versionWarn}>{versionWarnById[item.id].replace(/^⚠️ /, "")}</Text>
                </View>
              ) : null}
            </ListRow>
          );
        }}
      />
      <ChoiceSheet state={sheet} onClose={() => setSheet(null)} />
    </View>
  );
}

const makeStyles = (t: Theme) => StyleSheet.create({
  container: {
    flex: 1,
  },
  headerActions: {
    flexDirection: "row",
    alignItems: "center",
  },
  headerAction: {
    flexDirection: "row",
    alignItems: "center",
    gap: space.xs,
    height: TOUCH,
    paddingHorizontal: space.sm,
  },
  headerActionText: {
    color: t.accent,
    fontSize: font.size.sm,
    fontWeight: font.weight.bold,
  },
  presence: {
    flexDirection: "row",
    alignItems: "center",
    gap: space.xs,
    marginTop: space.xs,
  },
  dot: {
    width: 8,
    height: 8,
    borderRadius: radius.pill,
  },
  presenceText: {
    fontSize: font.size.xs,
    color: t.subtext,
  },
  meta: {
    fontSize: font.size.xs,
    color: t.subtext,
    marginTop: space.xs,
  },
  badges: {
    flexDirection: "row",
    flexWrap: "wrap",
    gap: space.sm,
    marginTop: space.sm,
  },
  versionWarn: {
    fontSize: font.size.xs,
    color: t.warning,
  },
});
