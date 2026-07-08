import { useEffect, useMemo, useRef, useState } from "react";
import {
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
  return { role: m.role === "user" ? "user" : "assistant", text: m.text, tools: m.tools };
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

  // Load prior transcript for a resumed session, then start the active poll if needed.
  useEffect(() => {
    if (!pc || !session) return;
    let cancelled = false;
    makeClient(pc.baseUrl, pc.token)
      .transcript(project, session, 0)
      .then((res) => {
        if (cancelled) return;
        nextLineRef.current = res.next;
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
      <ScrollView
        ref={scrollRef}
        style={styles.list}
        contentContainerStyle={styles.listContent}
        onContentSizeChange={() => {
          scrollRef.current?.scrollToEnd({ animated: didInitialScrollRef.current });
          didInitialScrollRef.current = true;
        }}
      >
        {chat.messages.map((m, i) => (
          <View key={i} style={m.role === "user" ? styles.userBubble : styles.assistantBubble}>
            {m.role === "assistant" && m.tools && m.tools.length > 0 ? (
              <View style={styles.chipRow}>
                {m.tools.map((t, ti) => (
                  <View key={ti} style={styles.chip}>
                    <Text style={styles.chipText}>{t}</Text>
                  </View>
                ))}
              </View>
            ) : null}
            <Text style={m.role === "user" ? styles.userText : styles.assistantText}>{m.text}</Text>
          </View>
        ))}

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
  list: {
    flex: 1,
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
  chipRow: {
    flexDirection: "row",
    flexWrap: "wrap",
    gap: 4,
    marginBottom: 4,
  },
  chip: {
    backgroundColor: "#dedede",
    borderRadius: 8,
    paddingHorizontal: 6,
    paddingVertical: 2,
  },
  chipText: {
    fontSize: 11,
    color: "#333",
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
