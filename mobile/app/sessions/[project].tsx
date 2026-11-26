import { useCallback, useEffect, useRef, useState, useMemo } from "react";
import {
  ActivityIndicator,
  FlatList,
  Platform,
  Pressable,
  RefreshControl,
  StyleSheet,
  Text,
  TextInput,
  ToastAndroid,
  View,
} from "react-native";
import { router, Stack, useFocusEffect, useLocalSearchParams } from "expo-router";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import { HttpError, isUnauthorized, makeClient } from "../../src/lib/api";
import { ChoiceSheet, type ChoiceSheetState } from "../../src/components/ChoiceSheet";
import { ScheduleSection } from "../../src/components/ScheduleSection";
import type { PC, SessionInfo } from "../../src/lib/types";
import { getPC, removePC } from "../../src/store/pcs";
import { font, radius, space, TOUCH, useTheme, type Theme } from "../../src/lib/theme";
import { Button, EmptyState, Icon, keepWords, ListRow, SheetFrame, StatusBadge } from "../../src/components/ui";
import { cleanupMessage, formatUpdated, isActiveSession as isActive, sessionPathParams } from "../../src/lib/sessions";


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
  const [sheet, setSheet] = useState<ChoiceSheetState | null>(null);
  const [ending, setEnding] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [q, setQ] = useState("");
  const [showArchived, setShowArchived] = useState(false);
  const [showPast, setShowPast] = useState(false);
  const [renaming, setRenaming] = useState<SessionInfo | null>(null);
  const [nameDraft, setNameDraft] = useState("");

  useEffect(() => {
    const id = setTimeout(() => setQ(query.trim()), 300);
    return () => clearTimeout(id);
  }, [query]);

  const load = useCallback(
    async (p: PC, isRefresh: boolean, silent = false) => {
      if (!silent) {
        if (isRefresh) setRefreshing(true);
        else setLoading(true);
        setError(null);
      }
      try {
        const list = await makeClient(p.baseUrl, p.token).sessions(project, { q, archived: showArchived });
        setSessions(list);
      } catch (e) {
        if (silent) return; // 주기 조회 실패는 화면을 오류로 바꾸지 않고 다음 주기에 맡긴다
        if (isUnauthorized(e)) {
          // Token revoked/invalid → drop this PC and send the user back to the PC list.
          await removePC(p.id);
          router.replace("/");
          return;
        }
        setError("세션을 불러오지 못했습니다. 다시 시도해 주세요.");
      } finally {
        if (silent) return;
        if (isRefresh) setRefreshing(false);
        else setLoading(false);
      }
    },
    [project, q, showArchived],
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

  // 대화에서 돌아오면 재조회(다른 목록 화면과 같게) — 첫 로드는 위 효과가 하므로 두 번째 포커스부터, 스피너는 당김새로고침 것만
  const reloadRef = useRef(() => {});
  reloadRef.current = () => { if (pc) load(pc, true); };
  const focusedRef = useRef(false);
  useFocusEffect(
    useCallback(() => {
      if (focusedRef.current) reloadRef.current();
      focusedRef.current = true;
    }, [])
  );

  // 「작업 중」(s.active) 세션이 있는 동안만, 화면이 보일 때만 10초마다 조용히 재조회 — 열어 둔 채로도 끝난 세션의 「작업 중」 이 풀린다
  const hasActive = sessions.some((s) => s.active);
  useFocusEffect(
    useCallback(() => {
      if (!pc || !hasActive) return;
      const iv = setInterval(() => load(pc, true, true), 10000);
      return () => clearInterval(iv);
    }, [pc, hasActive, load])
  );

  function handleRetry() {
    if (pc) load(pc, false);
  }

  function handleRefresh() {
    if (pc) load(pc, true);
  }

  function handleNewChat() {
    router.push({
      pathname: "/chat/[project]",
      params: { pc: pcId, project, path, fresh: "1" },
    });
  }

  function handlePress(item: SessionInfo) {
    router.push({
      pathname: "/chat/[project]",
      params: { pc: pcId, project, ...sessionPathParams(item, path), session: item.session_id },
    });
  }

  async function endLive(item: SessionInfo) {
    if (!pc) return;
    setEnding(item.session_id);
    try {
      const r = await makeClient(pc.baseUrl, pc.token).endSession(project, item.session_id);
      if (r.worktree && Platform.OS === "android") {
        ToastAndroid.show(cleanupMessage(r.worktree), ToastAndroid.LONG);
      } else if (!r.ended && Platform.OS === "android") {
        ToastAndroid.show("아직 종료되지 않았습니다. 잠시 후 다시 확인하세요", ToastAndroid.LONG);
      }
      await load(pc, true);
    } catch (e) {
      if (isUnauthorized(e)) {
        await removePC(pc.id);
        router.replace("/");
        return;
      }
      if (Platform.OS === "android") {
        ToastAndroid.show(
          e instanceof HttpError && e.status === 404 ? "이미 종료된 세션입니다" : "세션을 끝내지 못했습니다. 다시 시도해 주세요",
          ToastAndroid.LONG,
        );
      }
      await load(pc, true);
    } finally {
      setEnding(null);
    }
  }

  // 끝난 앱 대화가 쌓여 활성 세션이 묻히지 않게 — 검색·보관함은 찾는 중이라 접지 않는다
  const foldable = !q && !showArchived;
  const activeRows = sessions.filter(isActive);
  const pastRows = sessions.filter((s) => !isActive(s));
  const visible = !foldable ? sessions : showPast ? [...activeRows, ...pastRows] : activeRows;

  async function saveMeta(item: SessionInfo, body: { name?: string | null; archived?: boolean }) {
    if (!pc) return;
    try {
      const r = await makeClient(pc.baseUrl, pc.token).sessionMeta(project, item.session_id, body);
      if (r.worktree && Platform.OS === "android") ToastAndroid.show(cleanupMessage(r.worktree), ToastAndroid.LONG);
    } catch (e) {
      if (isUnauthorized(e)) {
        await removePC(pc.id);
        router.replace("/");
        return;
      }
      if (Platform.OS === "android") ToastAndroid.show("저장하지 못했습니다. 다시 시도해 주세요", ToastAndroid.LONG);
    }
    await load(pc, true);
  }

  function handleLongPress(item: SessionInfo) {
    const end = {
          label: "PC 터미널 세션 끝내기",
          // ChoiceSheet 가 선택 직후 자기를 닫으므로 다음 틱에 확인 시트를 연다.
          onPress: () =>
            setTimeout(
              () =>
                setSheet({
                  title: "정말 끝낼까요?",
                  message: "Mac 터미널에서 실행 중인 claude 가 종료됩니다. 터미널 창은 남습니다.",
                  choices: [{ label: "끝내기", onPress: () => endLive(item) }],
                }),
              0,
            ),
    };
    // 터미널에 안 열린 격리 세션 — 끝낼 프로세스는 없고 트리만 정리한다
    const endIsolated = {
      label: "세션 끝내기 · 격리 트리 정리",
      onPress: () =>
        setTimeout(
          () =>
            setSheet({
              title: "격리 트리를 정리할까요?",
              message: `변경이나 새 커밋이 없으면 트리와 브랜치 ${item.worktree?.branch ?? ""} 를 지웁니다. 있으면 남깁니다.`,
              choices: [{ label: "정리", onPress: () => endLive(item) }],
            }),
          0,
        ),
    };
    // 앱에서 연 세션은 끝나면 남는 프로세스가 없다 — 실행 중일 때만 멈출 수 있게
    const cancelRun = {
      label: "실행 취소",
      onPress: async () => {
        const c = makeClient(pc!.baseUrl, pc!.token);
        const r = await c.activeRun(project).catch(() => null);
        if (r?.run_id && r.session === item.session_id) await c.cancel(r.run_id).catch(() => null);
        else if (Platform.OS === "android") ToastAndroid.show("이 세션의 실행을 찾지 못했습니다", ToastAndroid.SHORT);
        await load(pc!, true);
      },
    };
    setSheet({
      title: item.name || item.preview || "(빈 대화)",
      choices: [
        {
          label: "이름 바꾸기",
          onPress: () => {
            setNameDraft(item.name ?? "");
            setRenaming(item);
          },
        },
        { label: item.archived ? "보관 해제" : "보관", onPress: () => saveMeta(item, { archived: !item.archived }) },
        ...(item.live ? [end] : item.worktree && !item.worktree.removed ? [endIsolated] : []),
        ...(item.app && item.active && !item.live ? [cancelRun] : []),
      ],
    });
  }

  // PC not resolved yet, or not found (redirect in flight): render nothing.
  if (!pc) {
    return <View style={styles.container} />;
  }

  return (
    <View style={styles.container}>
      <Stack.Screen
        options={{
          title: project,
          headerRight: () => (
            <Pressable
              style={({ pressed }) => [styles.headerAction, pressed ? { opacity: 0.7 } : null]}
              onPress={handleNewChat}
              accessibilityRole="button"
            >
              <Icon name="add" size={20} color={t.accent} />
              <Text style={styles.headerActionText}>새 대화</Text>
            </Pressable>
          ),
        }}
      />
      <View style={styles.searchWrap}>
        <Icon name="search" size={20} color={t.subtext} />
        <TextInput
          style={styles.searchInput}
          value={query}
          onChangeText={setQuery}
          placeholder="이름·질문·답 검색"
          placeholderTextColor={t.placeholder}
          clearButtonMode="while-editing"
        />
      </View>
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
          data={visible}
          keyExtractor={(item) => item.session_id}
          contentContainerStyle={{ paddingBottom: insets.bottom + space.md, flexGrow: 1 }}
          keyboardShouldPersistTaps="handled"
          ListHeaderComponent={foldable ? <ScheduleSection pc={pc} project={project} /> : null}
          refreshControl={<RefreshControl refreshing={refreshing} onRefresh={handleRefresh} colors={[t.accent]} progressBackgroundColor={t.surface} />}
          ListEmptyComponent={
            <EmptyState icon="forum" title={pastRows.length > 0 && foldable ? "활성 세션 없음" : "세션이 없습니다."} />
          }
          renderItem={({ item }) => (
            <ListRow card
              title={item.name || item.preview || "(빈 대화)"}
              onPress={() => handlePress(item)}
              onLongPress={() => handleLongPress(item)}
              right={
                ending === item.session_id ? (
                  <ActivityIndicator size="small" color={t.accent} />
                ) : item.active ? (
                  <StatusBadge status="working" />
                ) : item.waiting ? (
                  <StatusBadge status="waiting" />
                ) : null
              }
            >
              {item.live ? (
                <View style={styles.liveRow}>
                  <Icon name="terminal" size={14} color={t.accent} />
                  <Text style={styles.liveText} textBreakStrategy="balanced">{keepWords(item.channel ? "터미널 연결됨 — 보내면 그 터미널 대화에 바로 들어갑니다" : "PC 터미널에서 열림 — 보내면 갈라서 새 대화로 이어집니다")}</Text>
                </View>
              ) : null}
              {item.answer_preview ? (
                <Text style={styles.answerPreview} numberOfLines={1}>↳ {item.answer_preview}</Text>
              ) : null}
              <Text style={styles.updated}>{formatUpdated(item.updated)}</Text>
            </ListRow>
          )}
          ListFooterComponent={
            <View style={styles.footer}>
              {foldable && pastRows.length > 0 ? (
                <Button
                  variant="ghost"
                  icon={showPast ? "expand-less" : "expand-more"}
                  title={showPast ? "지난 대화 접기" : `지난 대화 ${pastRows.length}개`}
                  onPress={() => setShowPast((v) => !v)}
                />
              ) : null}
              <Button
                variant="ghost"
                icon="inventory-2"
                title={showArchived ? "보관함 닫기" : "보관함 보기"}
                onPress={() => setShowArchived((v) => !v)}
              />
            </View>
          }
          extraData={ending}
        />
      )}
      <ChoiceSheet state={sheet} onClose={() => setSheet(null)} />
      <SheetFrame visible={!!renaming} title="이름 바꾸기" variant="dialog" onClose={() => setRenaming(null)}>
        <TextInput
          style={styles.input}
          value={nameDraft}
          onChangeText={setNameDraft}
          placeholder="비우면 이름 제거"
          placeholderTextColor={t.placeholder}
          maxLength={80}
          autoFocus
        />
        <View style={styles.modalButtons}>
          <Button title="취소" variant="secondary" onPress={() => setRenaming(null)} style={styles.modalButton} />
          <Button
            title="저장"
            onPress={() => {
              const item = renaming;
              setRenaming(null);
              if (item) saveMeta(item, { name: nameDraft });
            }}
            style={styles.modalButton}
          />
        </View>
      </SheetFrame>
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
  searchWrap: {
    flexDirection: "row",
    alignItems: "center",
    gap: space.sm,
    marginHorizontal: space.md,
    marginBottom: space.sm,
    paddingHorizontal: space.md,
    height: TOUCH,
    borderRadius: radius.pill,
    borderWidth: 1,
    borderColor: t.inputBorder,
    backgroundColor: t.surface,
  },
  searchInput: {
    flex: 1,
    height: TOUCH,
    fontSize: font.size.md,
    color: t.text,
    paddingVertical: 0,
  },
  input: {
    minHeight: TOUCH,
    paddingHorizontal: space.md,
    borderRadius: radius.md,
    borderWidth: 1,
    borderColor: t.inputBorder,
    fontSize: font.size.md,
    color: t.text,
  },
  footer: {
    alignItems: "center",
    paddingVertical: space.sm,
  },
  modalButtons: {
    flexDirection: "row",
    gap: space.sm,
    paddingTop: space.sm,
    paddingBottom: space.lg,
  },
  modalButton: {
    flex: 1,
  },
  liveRow: {
    flexDirection: "row",
    alignItems: "center",
    gap: space.xs,
    marginTop: space.xxs,
  },
  liveText: {
    flex: 1,
    fontSize: font.size.xs,
    color: t.subtext,
  },
  answerPreview: {
    fontSize: font.size.sm,
    color: t.subtext,
    marginTop: space.xxs,
  },
  updated: {
    fontSize: font.size.xs,
    color: t.subtext,
    marginTop: space.xxs,
  },
});
