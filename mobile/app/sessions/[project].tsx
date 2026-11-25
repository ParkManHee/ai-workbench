import { useCallback, useEffect, useState, useMemo } from "react";
import {
  ActivityIndicator,
  Button,
  FlatList,
  Modal,
  Platform,
  Pressable,
  RefreshControl,
  StyleSheet,
  Text,
  TextInput,
  ToastAndroid,
  View,
} from "react-native";
import { router, Stack, useLocalSearchParams } from "expo-router";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import { HttpError, isUnauthorized, makeClient } from "../../src/lib/api";
import { ChoiceSheet, type ChoiceSheetState } from "../../src/components/ChoiceSheet";
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
    async (p: PC, isRefresh: boolean) => {
      if (isRefresh) setRefreshing(true);
      else setLoading(true);
      setError(null);
      try {
        const list = await makeClient(p.baseUrl, p.token).sessions(project, { q, archived: showArchived });
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
      params: { pc: pcId, project, path, session: item.session_id },
    });
  }

  async function endLive(item: SessionInfo) {
    if (!pc) return;
    setEnding(item.session_id);
    try {
      const r = await makeClient(pc.baseUrl, pc.token).endSession(project, item.session_id);
      if (!r.ended && Platform.OS === "android") {
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
  const isActive = (s: SessionInfo) => !!(s.live || s.active || s.waiting);
  const activeRows = sessions.filter(isActive);
  const pastRows = sessions.filter((s) => !isActive(s));
  const visible = !foldable ? sessions : showPast ? [...activeRows, ...pastRows] : activeRows;

  async function saveMeta(item: SessionInfo, body: { name?: string | null; archived?: boolean }) {
    if (!pc) return;
    try {
      await makeClient(pc.baseUrl, pc.token).sessionMeta(project, item.session_id, body);
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
        ...(item.live ? [end] : []),
      ],
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
      <TextInput
        style={styles.search}
        value={query}
        onChangeText={setQuery}
        placeholder="이름·질문·답 검색"
        placeholderTextColor={t.subtext}
        clearButtonMode="while-editing"
      />
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
          data={visible}
          keyExtractor={(item) => item.session_id}
          contentContainerStyle={{ paddingBottom: insets.bottom + 12, flexGrow: 1 }}
          refreshControl={<RefreshControl refreshing={refreshing} onRefresh={handleRefresh} />}
          ListEmptyComponent={
            <View style={styles.center}>
              <Text>{pastRows.length > 0 && foldable ? "활성 세션 없음" : "No sessions found."}</Text>
            </View>
          }
          renderItem={({ item }) => (
            <Pressable style={styles.row} onPress={() => handlePress(item)} onLongPress={() => handleLongPress(item)}>
              <View style={styles.rowTop}>
                <Text style={styles.preview} numberOfLines={1}>
                  {item.name || item.preview || "(빈 대화)"}
                </Text>
                {ending === item.session_id ? <ActivityIndicator size="small" /> : null}
                {item.active ? <Text style={styles.activeBadge}>🟢</Text> : item.waiting ? <Text style={styles.activeBadge}>🔴</Text> : null}
              </View>
              {item.live ? <Text style={styles.updated}>🖥 PC 터미널에서 열림 — 보내면 갈라서 새 대화로 이어집니다</Text> : null}
              {item.answer_preview ? (
                <Text style={styles.answerPreview} numberOfLines={1}>↳ {item.answer_preview}</Text>
              ) : null}
              <Text style={styles.updated}>{formatUpdated(item.updated)}</Text>
            </Pressable>
          )}
          ListFooterComponent={
            <>
              {foldable && pastRows.length > 0 ? (
                <Pressable style={styles.footer} onPress={() => setShowPast((v) => !v)}>
                  <Text style={styles.footerText}>{showPast ? "지난 대화 접기" : `지난 대화 ${pastRows.length}개`}</Text>
                </Pressable>
              ) : null}
              <Pressable style={styles.footer} onPress={() => setShowArchived((v) => !v)}>
                <Text style={styles.footerText}>{showArchived ? "보관함 닫기" : "보관함 보기"}</Text>
              </Pressable>
            </>
          }
          extraData={ending}
        />
      )}
      <ChoiceSheet state={sheet} onClose={() => setSheet(null)} />
      <Modal visible={!!renaming} transparent animationType="fade" onRequestClose={() => setRenaming(null)}>
        <Pressable style={styles.backdrop} onPress={() => setRenaming(null)}>
          <Pressable style={styles.modal} onPress={() => {}}>
            <Text style={styles.modalTitle}>이름 바꾸기</Text>
            <TextInput
              style={styles.search}
              value={nameDraft}
              onChangeText={setNameDraft}
              placeholder="비우면 이름 제거"
              placeholderTextColor={t.subtext}
              maxLength={80}
              autoFocus
            />
            <View style={styles.modalButtons}>
              <Button title="취소" onPress={() => setRenaming(null)} />
              <Button
                title="저장"
                onPress={() => {
                  const item = renaming;
                  setRenaming(null);
                  if (item) saveMeta(item, { name: nameDraft });
                }}
              />
            </View>
          </Pressable>
        </Pressable>
      </Modal>
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
  search: {
    marginHorizontal: 12,
    marginBottom: 8,
    paddingHorizontal: 12,
    paddingVertical: 8,
    borderWidth: StyleSheet.hairlineWidth,
    borderColor: t.border,
    borderRadius: 8,
    color: t.text,
  },
  footer: {
    padding: 16,
    alignItems: "center",
  },
  footerText: {
    color: t.accent,
    fontWeight: "600",
  },
  backdrop: {
    flex: 1,
    backgroundColor: "rgba(0,0,0,0.5)",
    justifyContent: "center",
    padding: 24,
  },
  modal: {
    backgroundColor: t.box,
    borderRadius: 12,
    paddingVertical: 16,
    gap: 12,
  },
  modalTitle: {
    fontSize: 17,
    fontWeight: "700",
    color: t.text,
    paddingHorizontal: 12,
  },
  modalButtons: {
    flexDirection: "row",
    justifyContent: "flex-end",
    gap: 12,
    paddingHorizontal: 12,
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
  answerPreview: {
    fontSize: 13,
    color: t.subtext,
    marginTop: 3,
  },
  updated: {
    fontSize: 12,
    color: t.subtext,
    marginTop: 2,
  },
});
