import { useEffect, useMemo, useRef, useState } from "react";
import {
  ActivityIndicator,
  Pressable,
  ScrollView,
  StyleSheet,
  Switch,
  Text,
  TextInput,
  View,
} from "react-native";
import { KeyboardStickyView, useKeyboardState } from "react-native-keyboard-controller";
import { router, useLocalSearchParams, Stack } from "expo-router";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import { isUnauthorized, makeClient, streamUrl } from "../../src/lib/api";
import { initialChatState, reduceEvent, verdictLabel } from "../../src/lib/events";
import type { ChatMsg, ChatState, PC, TranscriptMsg, WsEvent } from "../../src/lib/types";
import { getPC, removePC } from "../../src/store/pcs";

/** 데몬 트랜스크립트 항목 → 화면 채팅 메시지. role은 자유 문자열이라 user 외엔 assistant로 취급. */
function toChatMsg(m: TranscriptMsg): ChatMsg {
  return { role: m.role === "user" ? "user" : "assistant", text: m.text, tools: m.tools, toolDetails: m.tool_details };
}

interface DiffEntry {
  path: string;
  status: string;
}
interface DiffSummary {
  files: number;
  insertions: number;
  deletions: number;
  entries: DiffEntry[];
}

export default function Chat() {
  const { project, pc: pcId, path, session } = useLocalSearchParams<{
    project: string;
    pc: string;
    path: string;
    session?: string;
  }>();
  const insets = useSafeAreaInsets();
  // 키보드가 열리면 입력바가 키보드 위로 올라가므로, 하단 내비바 안전영역 패딩을
  // 빼서 입력바와 키보드 사이 흰 공백을 없앤다(닫혀 있을 때만 안전영역 적용).
  const kbVisible = useKeyboardState((s) => s.isVisible);
  // 키보드가 열리면 입력바가 리스트 위로 겹쳐 올라오므로, 가려지는 만큼 하단 패딩을 준다.
  const kbHeight = useKeyboardState((s) => s.height);
  const kbPad = kbVisible ? kbHeight : 0;
  // undefined = not checked yet, null = checked and no PC found (redirecting)
  const [pc, setPc] = useState<PC | null | undefined>(undefined);
  // initialChatState() has running:true by design (it's the state reset when a
  // run starts, per reduceEvent's tests); the idle screen before any send
  // should not show a "cancel"/disabled input, so start with running:false.
  const [chat, setChat] = useState<ChatState>(() => ({ ...initialChatState(), running: false }));
  const [prompt, setPrompt] = useState("");
  const [plan, setPlan] = useState(false);
  const [diff, setDiff] = useState<DiffSummary | null>(null);
  const [sendError, setSendError] = useState<string | null>(null);
  // 툴(Edit/Bash 등) 상세는 기본 접힘 — "🔧 작업 N" 버튼 탭 시에만 펼침(메시지 index 기준)
  const [expandedTools, setExpandedTools] = useState<Set<number>>(new Set());
  function toggleTools(i: number) {
    setExpandedTools((prev) => {
      const next = new Set(prev);
      if (next.has(i)) next.delete(i);
      else next.add(i);
      return next;
    });
  }

  const wsRef = useRef<WebSocket | null>(null);
  const runIdRef = useRef<string | null>(null);
  const doneRef = useRef(true); // true = no run currently in flight
  const reconnectedRef = useRef(false);
  const scrollRef = useRef<ScrollView>(null);
  // 최초 콘텐츠 렌더(과거 대화 로드 포함)는 즉시 맨 아래로 점프하고,
  // 이후(스트리밍 등)부터는 부드럽게 스크롤한다.
  const didInitialScrollRef = useRef(false);
  // Next transcript line offset to resume incremental polling from.
  const nextLineRef = useRef(0);
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);

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
  }, [pcId]);

  // Close any open socket when the screen unmounts (avoid leaked connections).
  useEffect(() => {
    return () => {
      wsRef.current?.close();
      wsRef.current = null;
    };
  }, []);

  // 키보드가 열리면 최신 메시지가 입력바에 가려지지 않게 맨아래로 스크롤.
  useEffect(() => {
    if (kbVisible) {
      const t = setTimeout(() => scrollRef.current?.scrollToEnd({ animated: true }), 50);
      return () => clearTimeout(t);
    }
  }, [kbVisible]);

  // Stop the active-session poll (unmount, or the user starts their own run).
  function stopPoll() {
    if (pollRef.current) {
      clearInterval(pollRef.current);
      pollRef.current = null;
    }
  }

  // Poll a resumed session that's actively running elsewhere (PC), appending
  // any new transcript lines. Skips a tick while a local run is in flight.
  const pollBusyRef = useRef(false);
  function startPoll(p: PC) {
    if (pollRef.current || !session) return;
    pollRef.current = setInterval(async () => {
      if (!doneRef.current) return; // local run in flight; let its WS drive the UI
      if (pollBusyRef.current) return; // 이전 틱이 아직 진행 중(대용량/느린 네트워크) — 겹치면 같은 offset을 두 번 읽어 메시지가 중복된다
      pollBusyRef.current = true;
      try {
        const res = await makeClient(p.baseUrl, p.token).transcript(project, session, nextLineRef.current);
        nextLineRef.current = res.next; // 항상 전진(필터로 메시지가 비어도 재파싱 중복 방지)
        if (res.messages.length > 0) {
          setChat((prev) => ({ ...prev, messages: [...prev.messages, ...res.messages.map(toChatMsg)] }));
        }
        if (!res.active) stopPoll();
      } catch (e) {
        if (isUnauthorized(e)) {
          stopPoll();
          await removePC(p.id);
          router.replace("/");
        }
        // else: transient network error, keep polling
      } finally {
        pollBusyRef.current = false;
      }
    }, 2000);
  }

  // 위로 스크롤 페이지네이션: prev(더 이전 페이지의 기준 line idx)와 로딩 가드.
  const prevRef = useRef<number | null>(null);
  const loadingOlderRef = useRef(false);
  const suppressAutoScrollRef = useRef(false);
  const [loadingOlder, setLoadingOlder] = useState(false); // 상단 스피너 표시용
  // 자동 스크롤은 "메시지 개수가 늘었을 때"만 — 툴 펼침/접힘 등 크기 변화로는 안 튀게.
  const lastMsgCountRef = useRef(0);
  // prepend 시 보던 위치 복원용(Android는 maintainVisibleContentPosition이 동작하지 않아 수동 보정).
  const scrollYRef = useRef(0);
  const contentHeightRef = useRef(0);
  // 위로 올라가 있을 때 맨아래로 점프하는 플로팅 버튼
  const [showJumpDown, setShowJumpDown] = useState(false);

  async function loadOlder() {
    if (!pc || !session || loadingOlderRef.current || prevRef.current == null) return;
    loadingOlderRef.current = true;
    setLoadingOlder(true);
    try {
      const res = await makeClient(pc.baseUrl, pc.token).transcriptBefore(project, session, prevRef.current);
      prevRef.current = res.prev;
      if (res.messages.length > 0) {
        suppressAutoScrollRef.current = true; // prepend 시 맨아래 자동 스크롤 금지
        setExpandedTools(new Set()); // index 기반 펼침 상태는 시프트되므로 리셋
        setChat((prev) => ({ ...prev, messages: [...res.messages.map(toChatMsg), ...prev.messages] }));
      }
    } catch {
      // best-effort; 다음 스크롤에서 재시도
    } finally {
      loadingOlderRef.current = false;
      setLoadingOlder(false);
    }
  }

  // Load recent transcript (최근 1시간) for a resumed session, then start the active poll if needed.
  useEffect(() => {
    if (!pc || !session) return;
    let cancelled = false;
    makeClient(pc.baseUrl, pc.token)
      .transcriptTail(project, session)
      .then((res) => {
        if (cancelled) return;
        nextLineRef.current = res.next;
        prevRef.current = res.prev;
        setChat({ messages: res.messages.map(toChatMsg), running: false, verdict: null, changedFiles: 0, error: null });
        if (res.active) startPoll(pc);
      })
      .catch(async (e) => {
        if (cancelled) return;
        if (isUnauthorized(e)) {
          await removePC(pc.id);
          router.replace("/");
        }
        // else: best-effort load; leave the chat empty and let the user send.
      });
    return () => {
      cancelled = true;
      stopPoll();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pc, session, project]);

  const client = useMemo(() => (pc ? makeClient(pc.baseUrl, pc.token) : null), [pc]);

  function connectWs(runId: string, p: PC) {
    doneRef.current = false;
    const ws = new WebSocket(streamUrl(p.baseUrl, runId, 0, p.token));
    wsRef.current = ws;

    ws.onmessage = (e: { data: string }) => {
      let ev: WsEvent;
      try {
        ev = JSON.parse(e.data);
      } catch {
        return; // ignore malformed frames
      }
      setChat((prev) => reduceEvent(prev, ev));
      if (ev.kind === "done") {
        doneRef.current = true;
        fetchDiff(p);
        ws.close();
      } else if (ev.kind === "error") {
        doneRef.current = true;
        ws.close();
      }
    };

    ws.onclose = () => {
      if (wsRef.current !== ws) return; // superseded by a newer run/reconnect
      if (doneRef.current) return;
      if (!reconnectedRef.current) {
        // v1 simplification: one reconnect attempt, offset=0 replay is acceptable.
        // Drop the partial assistant bubble first so the replayed tokens rebuild
        // it cleanly instead of doubling onto the text already shown.
        reconnectedRef.current = true;
        setChat((prev) => {
          const msgs = [...prev.messages];
          if (msgs.at(-1)?.role === "assistant") msgs.pop();
          return { ...initialChatState(), messages: msgs };
        });
        connectWs(runId, p);
        return;
      }
      doneRef.current = true;
      setChat((prev) => ({ ...prev, running: false, error: prev.error ?? "연결이 끊겼습니다." }));
    };

    ws.onerror = () => {
      // onclose fires right after; nothing to log (never log token/url).
    };
  }

  async function fetchDiff(p: PC) {
    if (!path) return;
    try {
      const d: DiffSummary = await makeClient(p.baseUrl, p.token).diff(path);
      setDiff(d);
    } catch {
      // git summary is best-effort; ignore failures.
    }
  }

  async function handleSend() {
    if (!pc || !client || !project) return;
    const text = prompt.trim();
    if (!text || chat.running) return;

    stopPoll(); // the user's own run now drives the live view
    setSendError(null);
    setDiff(null);
    setPrompt("");
    reconnectedRef.current = false;

    const userMsg: ChatMsg = { role: "user", text };
    setChat((prev) => ({ ...initialChatState(), messages: [...prev.messages, userMsg] }));

    try {
      const { run_id } = await client.chat(project, text, plan, session);
      runIdRef.current = run_id;
      connectWs(run_id, pc);
    } catch (e) {
      if (isUnauthorized(e)) {
        // Token revoked/invalid → drop this PC and send the user back to the PC list.
        await removePC(pc.id);
        router.replace("/");
        return;
      }
      setSendError("전송 실패. 다시 시도해주세요.");
      setChat((prev) => ({ ...prev, running: false }));
    }
  }

  function handleCancel() {
    const runId = runIdRef.current;
    if (!pc || !runId) return;
    // Fire-and-forget: the run's WS stream will still emit a terminal `done`
    // event once the process is killed, which drives the rest of the UI.
    makeClient(pc.baseUrl, pc.token)
      .cancel(runId)
      .catch(() => {});
  }

  if (!pc) {
    return (
      <>
        <Stack.Screen options={{ title: project ?? "실행" }} />
        <View style={styles.container} />
      </>
    );
  }

  const running = chat.running;

  return (
    <View style={styles.container}>
      <Stack.Screen options={{ title: project ?? "실행" }} />
      {session && path ? (
        <View style={styles.resumeBar}>
          <Text style={styles.resumeLabel}>PC에서 이어받기:</Text>
          <Text style={styles.resumeCmd} selectable>
            {`cd ${path} && claude --resume ${session}`}
          </Text>
        </View>
      ) : null}
      <View style={styles.listWrap}>
      <ScrollView
        ref={scrollRef}
        style={styles.list}
        contentContainerStyle={[styles.listContent, kbPad > 0 ? { paddingBottom: kbPad + 12 } : null]}
        scrollEventThrottle={100}
        onScroll={(e) => {
          const { contentOffset, contentSize, layoutMeasurement } = e.nativeEvent;
          scrollYRef.current = contentOffset.y;
          // 맨아래에서 충분히 올라가 있으면 ⬇ 버튼 표시
          setShowJumpDown(contentSize.height - layoutMeasurement.height - contentOffset.y > 400);
          // 맨 위 근처로 스크롤하면 이전 대화 페이지 로드.
          // 단, 최초 포커스(맨아래 점프) 전에는 발동 금지 — 진입 직후 y=0에서 오발동해 포커스를 망침.
          if (didInitialScrollRef.current && contentOffset.y <= 30) loadOlder();
        }}
        onContentSizeChange={(_w, h) => {
          const prevHeight = contentHeightRef.current;
          contentHeightRef.current = h;
          const count = chat.messages.length;
          const grew = count > lastMsgCountRef.current;
          lastMsgCountRef.current = count;
          if (suppressAutoScrollRef.current) {
            // 이전 페이지 prepend — 보던 위치를 수동 복원해 연속 위스크롤이 끊기지 않게 한다
            suppressAutoScrollRef.current = false;
            scrollRef.current?.scrollTo({ y: h - prevHeight + scrollYRef.current, animated: false });
            return;
          }
          if (!didInitialScrollRef.current) {
            // 최초 로드: 즉시 맨아래로 점프(+레이아웃 안정화 후 한 번 더)
            if (count === 0) return; // 아직 콘텐츠 없음 — 포커스 완료로 치지 않는다
            scrollRef.current?.scrollToEnd({ animated: false });
            setTimeout(() => scrollRef.current?.scrollToEnd({ animated: false }), 80);
            didInitialScrollRef.current = true;
            return;
          }
          // 이후: 메시지가 늘었을 때만(스트리밍/폴 수신) 아래로 — 툴 펼침/접힘엔 안 움직임
          if (grew) scrollRef.current?.scrollToEnd({ animated: true });
        }}
      >
        {loadingOlder ? <ActivityIndicator style={{ marginVertical: 8 }} /> : null}
        {chat.messages.map((m, i) => {
          const hasTools = m.role === "assistant" && m.tools && m.tools.length > 0;
          const hasText = m.text.trim().length > 0;
          const isOpen = expandedTools.has(i);
          const details = m.toolDetails && m.toolDetails.length > 0 ? m.toolDetails : m.tools ?? [];
          // 툴만 있는 항목(답변 텍스트 없음): 말풍선 대신 작은 접힘 버튼만
          if (m.role === "assistant" && hasTools && !hasText) {
            return (
              <View key={i} style={styles.toolOnlyRow}>
                <Pressable style={styles.toolButton} onPress={() => toggleTools(i)}>
                  <Text style={styles.toolButtonText}>
                    🔧 작업 {m.tools!.length} {isOpen ? "▲" : "▼"}
                  </Text>
                </Pressable>
                {isOpen
                  ? details.map((d, di) => (
                      <Text key={di} style={styles.toolDetail} selectable>
                        {d}
                      </Text>
                    ))
                  : null}
              </View>
            );
          }
          return (
            <View key={i} style={m.role === "user" ? styles.userBubble : styles.assistantBubble}>
              {hasTools ? (
                <Pressable style={styles.toolButton} onPress={() => toggleTools(i)}>
                  <Text style={styles.toolButtonText}>
                    🔧 작업 {m.tools!.length} {isOpen ? "▲" : "▼"}
                  </Text>
                </Pressable>
              ) : null}
              {hasTools && isOpen
                ? details.map((d, di) => (
                    <Text key={di} style={styles.toolDetail} selectable>
                      {d}
                    </Text>
                  ))
                : null}
              {hasText ? (
                <Text style={m.role === "user" ? styles.userText : styles.assistantText}>{m.text}</Text>
              ) : null}
            </View>
          );
        })}

        {!chat.running && chat.verdict ? (
          <View style={styles.verdictRow}>
            <Text style={styles.verdictText}>{verdictLabel(chat.verdict)}</Text>
            <Text style={styles.changedFiles}>변경 파일 {chat.changedFiles}개</Text>
          </View>
        ) : null}

        {chat.error ? <Text style={styles.errorText}>⚠️ {chat.error}</Text> : null}

        {diff ? (
          <View style={styles.diffBox}>
            <Text style={styles.diffTitle}>
              git 변경: 파일 {diff.files}개 · +{diff.insertions} / -{diff.deletions}
            </Text>
            {diff.entries.map((e, i) => (
              <Text key={i} style={styles.diffEntry}>
                {e.status} {e.path}
              </Text>
            ))}
          </View>
        ) : null}
      </ScrollView>
      {showJumpDown ? (
        <Pressable
          style={[styles.jumpDownButton, kbPad > 0 ? { bottom: 14 + kbPad } : null]}
          onPress={() => scrollRef.current?.scrollToEnd({ animated: true })}
        >
          <Text style={styles.jumpDownText}>⬇</Text>
        </Pressable>
      ) : null}
      </View>

      {sendError ? <Text style={styles.errorText}>{sendError}</Text> : null}

      <KeyboardStickyView>
        <View style={[styles.inputBar, { paddingBottom: kbVisible ? 8 : insets.bottom + 8 }]}>
          <View style={styles.planRow}>
          <Text style={styles.planLabel}>plan</Text>
          <Switch
            value={plan}
            onValueChange={setPlan}
            disabled={running}
            style={{ transform: [{ scale: 0.85 }] }}
          />
        </View>
        <TextInput
          style={styles.input}
          multiline
          value={prompt}
          onChangeText={setPrompt}
          editable={!running}
          placeholder="메시지를 입력하세요"
          placeholderTextColor="#333"
        />
        {running ? (
          <Pressable style={styles.cancelButton} onPress={handleCancel}>
            <Text style={styles.buttonText}>취소</Text>
          </Pressable>
        ) : (
          <Pressable
            style={styles.sendButton}
            onPress={handleSend}
            disabled={!prompt.trim()}
          >
            <Text style={styles.buttonText}>전송</Text>
          </Pressable>
        )}
        </View>
      </KeyboardStickyView>
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
  },
  resumeBar: {
    paddingHorizontal: 12,
    paddingVertical: 6,
    backgroundColor: "#f7f7f7",
    borderBottomWidth: StyleSheet.hairlineWidth,
    borderBottomColor: "#ccc",
    gap: 2,
  },
  resumeLabel: {
    fontSize: 11,
    color: "#666",
  },
  resumeCmd: {
    fontSize: 12,
    fontFamily: "monospace",
    color: "#222",
  },
  listWrap: {
    flex: 1,
  },
  list: {
    flex: 1,
  },
  jumpDownButton: {
    position: "absolute",
    right: 14,
    bottom: 14,
    width: 40,
    height: 40,
    borderRadius: 20,
    backgroundColor: "rgba(0,0,0,0.45)",
    alignItems: "center",
    justifyContent: "center",
  },
  jumpDownText: {
    color: "white",
    fontSize: 18,
  },
  listContent: {
    padding: 12,
    gap: 8,
  },
  userBubble: {
    alignSelf: "flex-end",
    backgroundColor: "#dcefff",
    borderRadius: 12,
    padding: 10,
    marginVertical: 4,
    maxWidth: "85%",
  },
  assistantBubble: {
    alignSelf: "flex-start",
    backgroundColor: "#f0f0f0",
    borderRadius: 12,
    padding: 10,
    marginVertical: 4,
    maxWidth: "85%",
  },
  userText: {
    fontSize: 15,
  },
  assistantText: {
    fontSize: 15,
  },
  toolOnlyRow: {
    alignSelf: "flex-start",
    maxWidth: "85%",
    marginVertical: 2,
  },
  toolButton: {
    alignSelf: "flex-start",
    backgroundColor: "#e6e6e6",
    borderRadius: 8,
    paddingHorizontal: 8,
    paddingVertical: 3,
    marginBottom: 4,
  },
  toolButtonText: {
    fontSize: 11,
    color: "#555",
  },
  toolDetail: {
    fontSize: 11,
    color: "#444",
    fontFamily: "monospace",
    backgroundColor: "#f2f2f2",
    borderRadius: 6,
    paddingHorizontal: 6,
    paddingVertical: 3,
    marginBottom: 3,
  },
  verdictRow: {
    alignItems: "center",
    paddingVertical: 8,
    gap: 2,
  },
  verdictText: {
    fontWeight: "600",
  },
  changedFiles: {
    fontSize: 12,
    color: "#666",
  },
  errorText: {
    color: "#c0392b",
    textAlign: "center",
    paddingVertical: 4,
  },
  diffBox: {
    backgroundColor: "#f7f7f7",
    borderRadius: 8,
    padding: 10,
    marginTop: 4,
  },
  diffTitle: {
    fontWeight: "600",
    marginBottom: 4,
  },
  diffEntry: {
    fontSize: 12,
    color: "#444",
  },
  inputBar: {
    flexDirection: "row",
    alignItems: "flex-end",
    padding: 8,
    gap: 8,
    borderTopWidth: StyleSheet.hairlineWidth,
    borderTopColor: "#ccc",
  },
  planRow: {
    flexDirection: "row",
    alignItems: "center",
    gap: 4,
    height: 40,
  },
  planLabel: {
    fontSize: 13,
    color: "#333",
  },
  input: {
    flex: 1,
    borderWidth: StyleSheet.hairlineWidth,
    borderColor: "#ccc",
    borderRadius: 8,
    paddingHorizontal: 10,
    paddingVertical: 6,
    maxHeight: 120,
    color: "#111",
  },
  sendButton: {
    backgroundColor: "#2f6fed",
    borderRadius: 8,
    paddingHorizontal: 16,
    paddingVertical: 10,
  },
  cancelButton: {
    backgroundColor: "#c0392b",
    borderRadius: 8,
    paddingHorizontal: 16,
    paddingVertical: 10,
  },
  buttonText: {
    color: "white",
    fontWeight: "600",
  },
});
