import { useCallback, useMemo, useState } from "react";
import { RefreshControl, ScrollView, StyleSheet, Text, View } from "react-native";
import { router, useFocusEffect } from "expo-router";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import { makeClient } from "../src/lib/api";
import { fetchNeeds, NEEDS_POLL_MS, type PcNeeds } from "../src/lib/needs";
import { permView } from "../src/lib/permSummary";
import { formatUpdated, sessionPathParams } from "../src/lib/sessions";
import { loadPCs } from "../src/store/pcs";
import { font, radius, space, useTheme, type Theme } from "../src/lib/theme";
import { Button, EmptyState, Icon, ListRow } from "../src/components/ui";

/** 권한 요청 한 줄 요약 — 채팅 화면 권한 카드와 같은 permView 를 쓴다 */
function permLine(tool: string, summary: string): string {
  const v = permView(tool, summary);
  switch (v.kind) {
    case "file": return v.path;
    case "command": return `$ ${v.command}`;
    case "plan": return v.plan;
    case "question": return v.questions[0]?.question ?? summary;
    default: return summary;
  }
}

/** 모든 PC·프로젝트의 권한 요청과 답 대기 대화 — 권한은 여기서 답하고, 대화는 눌러서 들어간다 */
export default function Needs() {
  const t = useTheme();
  const styles = useMemo(() => makeStyles(t), [t]);
  const insets = useSafeAreaInsets();
  const [list, setList] = useState<PcNeeds[] | undefined>(undefined);
  const [refreshing, setRefreshing] = useState(false);

  const load = useCallback(async () => {
    setList(await fetchNeeds(await loadPCs()));
  }, []);

  useFocusEffect(
    useCallback(() => {
      load();
      const iv = setInterval(load, NEEDS_POLL_MS);
      return () => clearInterval(iv);
    }, [load])
  );

  async function handleRefresh() {
    setRefreshing(true);
    await load();
    setRefreshing(false);
  }

  async function answer(pc: PcNeeds["pc"], id: string, allow: boolean) {
    // 먼저 지워 두 번 누르지 않게 하고, 결과(이미 답했거나 만료 404 포함)는 다시 불러 맞춘다
    setList((cur) => cur?.map((x) => x.pc.id === pc.id && x.data
      ? { ...x, data: { ...x.data, permissions: x.data.permissions.filter((p) => p.id !== id) } } : x));
    try {
      await makeClient(pc.baseUrl, pc.token).permissionAnswer(id, allow);
    } finally {
      load();
    }
  }

  const multiPc = (list?.length ?? 0) > 1;
  const perms = (list ?? []).flatMap((x) => (x.data?.permissions ?? []).map((p) => ({ pc: x.pc, p })));
  const waiting = (list ?? []).flatMap((x) => (x.data?.waiting ?? []).map((s) => ({ pc: x.pc, s })))
    .sort((a, b) => b.s.updated - a.s.updated);
  const offline = (list ?? []).filter((x) => x.reason === "offline").map((x) => x.pc.label);
  const old = (list ?? []).filter((x) => x.reason === "old").map((x) => x.pc.label);

  return (
    <ScrollView
      contentContainerStyle={{ paddingBottom: insets.bottom + space.md, flexGrow: 1 }}
      refreshControl={<RefreshControl refreshing={refreshing} onRefresh={handleRefresh} colors={[t.accent]} progressBackgroundColor={t.surface} />}
    >
      {list !== undefined && perms.length === 0 && waiting.length === 0 ? (
        <EmptyState icon="check-circle-outline" title="답할 것이 없습니다." />
      ) : null}

      {perms.length > 0 ? <Text style={styles.section}>권한 요청 {perms.length}</Text> : null}
      {perms.map(({ pc, p }) => (
        <View key={`${pc.id}:${p.id}`} style={styles.perm}>
          <View style={styles.permHead}>
            <Icon name="lock" size={18} color={t.warning} />
            <Text style={styles.permTitle} numberOfLines={1}>{p.tool_name} · {p.project}{multiPc ? ` · ${pc.label}` : ""}</Text>
          </View>
          <Text style={styles.permBody} numberOfLines={3}>{permLine(p.tool_name, p.summary)}</Text>
          <View style={styles.permButtons}>
            <Button title="허용" style={styles.permButton} onPress={() => answer(pc, p.id, true)} />
            <Button title="거부" variant="danger" style={styles.permButton} onPress={() => answer(pc, p.id, false)} />
          </View>
        </View>
      ))}

      {waiting.length > 0 ? <Text style={styles.section}>답 기다리는 대화 {waiting.length}</Text> : null}
      {waiting.map(({ pc, s }) => (
        <ListRow
          key={`${pc.id}:${s.project}:${s.session_id}`}
          title={s.name || s.preview || "제목 없는 대화"}
          subtitle={`${s.project}${multiPc ? ` · ${pc.label}` : ""} · ${formatUpdated(s.updated)}`}
          onPress={() => router.push({ pathname: "/chat/[project]", params: { pc: pc.id, project: s.project, ...sessionPathParams(s, s.path), session: s.session_id } })}
          right={<Icon name="chevron-right" size={24} color={t.subtext} />}
        >
          {s.answer_preview ? <Text style={styles.question} numberOfLines={2}>{s.answer_preview}</Text> : null}
        </ListRow>
      ))}

      {offline.length > 0 ? (
        <View style={styles.note}>
          <Icon name="cloud-off" size={16} color={t.subtext} />
          <Text style={styles.noteText}>응답 없는 PC: {offline.join(", ")} — 이 PC 의 요청은 빠져 있습니다</Text>
        </View>
      ) : null}
      {old.length > 0 ? (
        <View style={styles.note}>
          <Icon name="warning-amber" size={16} color={t.warning} />
          <Text style={styles.noteText}>데몬 업데이트 필요: {old.join(", ")} — 이 PC 는 목록을 줄 수 없습니다</Text>
        </View>
      ) : null}
    </ScrollView>
  );
}

const makeStyles = (t: Theme) => StyleSheet.create({
  section: {
    fontSize: font.size.sm,
    fontWeight: font.weight.bold,
    color: t.subtext,
    paddingHorizontal: space.lg,
    paddingTop: space.lg,
    paddingBottom: space.sm,
  },
  perm: {
    marginHorizontal: space.lg,
    marginBottom: space.md,
    padding: space.md,
    gap: space.sm,
    borderRadius: radius.md,
    backgroundColor: t.surface,
  },
  permHead: {
    flexDirection: "row",
    alignItems: "center",
    gap: space.xs,
  },
  permTitle: {
    flex: 1,
    fontSize: font.size.md,
    fontWeight: font.weight.bold,
    color: t.text,
  },
  permBody: {
    fontSize: font.size.sm,
    color: t.subtext,
  },
  permButtons: {
    flexDirection: "row",
    gap: space.sm,
  },
  permButton: {
    flex: 1,
  },
  question: {
    fontSize: font.size.sm,
    color: t.text,
    marginTop: space.xs,
  },
  note: {
    flexDirection: "row",
    alignItems: "center",
    gap: space.xs,
    paddingHorizontal: space.lg,
    paddingTop: space.md,
  },
  noteText: {
    flex: 1,
    fontSize: font.size.xs,
    color: t.subtext,
  },
});
