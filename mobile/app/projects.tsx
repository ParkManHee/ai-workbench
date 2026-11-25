import { useCallback, useEffect, useRef, useState, useMemo } from "react";
import {
  ActivityIndicator,
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
import type { PC, Preflight, Project, SessionInfo } from "../src/lib/types";
import { getPC, removePC } from "../src/store/pcs";
import { font, space, TOUCH, useTheme, type Theme } from "../src/lib/theme";
import { Button, EmptyState, Icon, ListRow, StatusBadge } from "../src/components/ui";
import { formatUpdated, isActiveSession, sessionPathParams } from "../src/lib/sessions";

/** 프로젝트 아래에 붙이는 지난 대화 수 — 활성 세션은 개수와 무관하게 전부 보인다 */
const RECENT = 3;

type Item =
  | { kind: "project"; project: Project }
  | { kind: "session"; project: Project; session: SessionInfo }
  | { kind: "more"; project: Project; total: number }
  | { kind: "fold"; count: number };

/** 보이는 세션이 있는 프로젝트는 위에 세션과 함께, 없는 프로젝트는 맨 아래 접기 줄 뒤로 */
function buildItems(projects: Project[], showIdle: boolean): Item[] {
  const items: Item[] = [];
  const idle: Project[] = [];
  for (const project of projects) {
    // 옛 데몬은 sessions 를 보내지 않는다 — 알 수 없으니 접지 않고 예전처럼 보인다
    if (project.sessions === undefined) {
      items.push({ kind: "project", project });
      continue;
    }
    if (project.sessions.length === 0) {
      idle.push(project);
      continue;
    }
    const active = project.sessions.filter(isActiveSession);
    const past = project.sessions.filter((s) => !isActiveSession(s)).slice(0, RECENT);
    items.push({ kind: "project", project });
    for (const session of [...active, ...past]) items.push({ kind: "session", project, session });
    if (project.sessions.length > active.length + past.length) {
      items.push({ kind: "more", project, total: project.sessions.length });
    }
  }
  // 세션 있는 프로젝트가 하나도 없으면 접어 둘 이유가 없다 — 접기 줄 없이 전부 보인다
  if (idle.length > 0 && items.length === 0) {
    for (const project of idle) items.push({ kind: "project", project });
  } else if (idle.length > 0) {
    items.push({ kind: "fold", count: idle.length });
    if (showIdle) for (const project of idle) items.push({ kind: "project", project });
  }
  return items;
}

function itemKey(item: Item): string {
  switch (item.kind) {
    case "project": return `p:${item.project.name}`;
    case "session": return `s:${item.project.name}:${item.session.session_id}`;
    case "more": return `m:${item.project.name}`;
    case "fold": return "fold";
  }
}

function badgeText(project: Project): string | null {
  const b = project.badge;
  if (!b) return null;
  return `할 일 ${b.todo} · 진행 ${b.doing} · 완료 ${b.done}`;
}

function preflightText(preflight: Preflight | null): string {
  if (!preflight) return "";
  if (preflight.claude_path) return "Claude 준비됨";
  const failing = preflight.checks.find((c) => !c.ok);
  return failing ? `준비 안 됨: ${failing.detail}` : "Claude 경로를 찾지 못했습니다";
}

export default function Projects() {
  const t = useTheme();
  const styles = useMemo(() => makeStyles(t), [t]);
  const { pc: pcId } = useLocalSearchParams<{ pc: string }>();
  const insets = useSafeAreaInsets();
  // undefined = not checked yet, null = checked and no PC found (redirecting)
  const [pc, setPc] = useState<PC | null | undefined>(undefined);
  const [projects, setProjects] = useState<Project[]>([]);
  const [preflight, setPreflight] = useState<Preflight | null>(null);
  const [loading, setLoading] = useState(false);
  const [refreshing, setRefreshing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [showIdle, setShowIdle] = useState(false);
  const items = useMemo(() => buildItems(projects, showIdle), [projects, showIdle]);

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
      setError("프로젝트를 불러오지 못했습니다. 다시 시도해 주세요.");
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

  function handleSessionPress(project: Project, session: SessionInfo) {
    router.push({
      pathname: "/chat/[project]",
      params: { pc: pcId, project: project.name, ...sessionPathParams(session, project.path), session: session.session_id },
    });
  }

  function renderItem({ item }: { item: Item }) {
    if (item.kind === "fold") {
      return (
        <View style={styles.fold}>
          <Button
            variant="ghost"
            icon={showIdle ? "expand-less" : "expand-more"}
            title={showIdle ? "세션 없는 프로젝트 접기" : `세션 없는 프로젝트 ${item.count}개`}
            onPress={() => setShowIdle((v) => !v)}
          />
        </View>
      );
    }
    if (item.kind === "more") {
      return (
        <Pressable
          onPress={() => handlePress(item.project)}
          accessibilityRole="button"
          accessibilityLabel={`${item.project.name} 세션 전체 ${item.total}개`}
          style={({ pressed }) => [styles.child, styles.more, pressed ? { backgroundColor: t.surface } : null]}
        >
          <Text style={styles.moreText}>전체 {item.total}개</Text>
          <Icon name="chevron-right" size={20} color={t.accent} />
        </Pressable>
      );
    }
    if (item.kind === "session") {
      const s = item.session;
      return (
        <Pressable
          onPress={() => handleSessionPress(item.project, s)}
          accessibilityRole="button"
          style={({ pressed }) => [styles.child, pressed ? { backgroundColor: t.surface } : null]}
        >
          <View style={styles.childBody}>
            <Text style={styles.childTitle} numberOfLines={1}>{s.name || s.preview || "(빈 대화)"}</Text>
            <View style={styles.childMeta}>
              {s.live ? <Icon name="terminal" size={14} color={t.accent} /> : null}
              <Text style={styles.childMetaText} numberOfLines={1}>
                {s.live ? "PC 터미널 · " : ""}{formatUpdated(s.updated)}
              </Text>
            </View>
          </View>
          {s.active ? <StatusBadge status="working" /> : s.waiting ? <StatusBadge status="waiting" /> : null}
        </Pressable>
      );
    }
    const project = item.project;
    const badge = badgeText(project);
    return (
      <ListRow
        title={project.name}
        subtitle={project.path}
        subtitleEllipsize="middle"
        onPress={() => handlePress(project)}
        right={
          <View style={styles.right}>
            {project.agent_status ? <StatusBadge status={project.agent_status} /> : null}
            <Icon name="chevron-right" size={24} color={t.subtext} />
          </View>
        }
      >
        {badge ? <Text style={styles.badge}>{badge}</Text> : null}
      </ListRow>
    );
  }

  // PC not resolved yet, or not found (redirect in flight): render nothing.
  if (!pc) {
    return <View style={styles.container} />;
  }

  return (
    <View style={styles.container}>
      <Stack.Screen options={{ title: pc.label }} />
      {preflight ? (
        <View style={[styles.preflightBanner, !preflight.claude_path ? { backgroundColor: t.surfaceAlt } : null]}>
          <Icon
            name={preflight.claude_path ? "check-circle" : "warning-amber"}
            size={16}
            color={preflight.claude_path ? t.success : t.warning}
          />
          <Text style={styles.preflightText}>{preflightText(preflight)}</Text>
        </View>
      ) : null}
      {loading ? (
        <View style={styles.center}>
          <ActivityIndicator color={t.accent} />
        </View>
      ) : error ? (
        <EmptyState
          icon="error-outline"
          title={error}
          action={<Button title="다시 시도" icon="refresh" variant="secondary" onPress={handleRetry} />}
        />
      ) : (
        <FlatList
          data={items}
          keyExtractor={itemKey}
          contentContainerStyle={{ paddingBottom: insets.bottom + space.md, flexGrow: 1 }}
          refreshControl={<RefreshControl refreshing={refreshing} onRefresh={handleRefresh} colors={[t.accent]} progressBackgroundColor={t.surface} />}
          ListEmptyComponent={<EmptyState icon="folder-open" title="프로젝트가 없습니다." />}
          renderItem={renderItem}
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
    padding: space.xl,
  },
  preflightBanner: {
    flexDirection: "row",
    alignItems: "center",
    gap: space.sm,
    paddingHorizontal: space.lg,
    paddingVertical: space.sm,
    backgroundColor: t.surface,
  },
  preflightText: {
    flex: 1,
    fontSize: font.size.xs,
    color: t.text,
  },
  right: {
    flexDirection: "row",
    alignItems: "center",
    gap: space.xs,
  },
  badge: {
    fontSize: font.size.xs,
    color: t.subtext,
    marginTop: space.xs,
  },
  // 프로젝트 아래 세션 줄 — 들여 써서 어느 프로젝트 것인지 보이게
  child: {
    flexDirection: "row",
    alignItems: "center",
    gap: space.md,
    minHeight: TOUCH,
    paddingLeft: space.xl + space.lg,
    paddingRight: space.lg,
    paddingVertical: space.sm,
    borderBottomWidth: StyleSheet.hairlineWidth,
    borderBottomColor: t.border,
  },
  childBody: {
    flex: 1,
    gap: space.xxs,
  },
  childTitle: {
    fontSize: font.size.sm,
    color: t.text,
  },
  childMeta: {
    flexDirection: "row",
    alignItems: "center",
    gap: space.xs,
  },
  childMetaText: {
    flexShrink: 1,
    fontSize: font.size.xs,
    color: t.subtext,
  },
  more: {
    gap: space.xs,
  },
  moreText: {
    flex: 1,
    fontSize: font.size.sm,
    fontWeight: font.weight.bold,
    color: t.accent,
  },
  fold: {
    alignItems: "center",
    paddingVertical: space.sm,
  },
});
