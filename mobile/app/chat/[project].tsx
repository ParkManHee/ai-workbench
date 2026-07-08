import { useEffect, useMemo, useRef, useState } from "react";
import {
  Keyboard,
  Pressable,
  ScrollView,
  StyleSheet,
  Switch,
  Text,
  TextInput,
  View,
} from "react-native";
import { router, useLocalSearchParams, Stack } from "expo-router";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import { isUnauthorized, makeClient, streamUrl } from "../../src/lib/api";
import { initialChatState, reduceEvent, verdictLabel } from "../../src/lib/events";
import type { ChatMsg, ChatState, WsEvent } from "../../src/lib/types";
import { clearSession, loadSession, type Session } from "../../src/store/session";

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
  const { project, path } = useLocalSearchParams<{ project: string; path: string }>();
  const insets = useSafeAreaInsets();
  // undefined = not checked yet, null = checked and no session (redirecting)
  const [session, setSession] = useState<Session | null | undefined>(undefined);
  // initialChatState() has running:true by design (it's the state reset when a
  // run starts, per reduceEvent's tests); the idle screen before any send
  // should not show a "cancel"/disabled input, so start with running:false.
  const [chat, setChat] = useState<ChatState>(() => ({ ...initialChatState(), running: false }));
  const [prompt, setPrompt] = useState("");
  const [plan, setPlan] = useState(false);
  const [diff, setDiff] = useState<DiffSummary | null>(null);
  const [sendError, setSendError] = useState<string | null>(null);
  // 키보드 높이를 직접 추적해 입력바를 그만큼 올린다(edge-to-edge Android에서
  // KeyboardAvoidingView가 네이티브 헤더와 오작동하는 문제 회피).
  const [kbHeight, setKbHeight] = useState(0);

  const wsRef = useRef<WebSocket | null>(null);
  const runIdRef = useRef<string | null>(null);
  const doneRef = useRef(true); // true = no run currently in flight
  const reconnectedRef = useRef(false);
  const scrollRef = useRef<ScrollView>(null);

  useEffect(() => {
    let cancelled = false;
    loadSession().then((s) => {
      if (cancelled) return;
      if (!s) {
        router.replace("/pair");
        return;
      }
      setSession(s);
    });
    return () => {
      cancelled = true;
    };
  }, []);

  // Close any open socket when the screen unmounts (avoid leaked connections).
  useEffect(() => {
    return () => {
      wsRef.current?.close();
      wsRef.current = null;
    };
  }, []);

  // 키보드 표시/숨김에 따라 입력바를 올릴 높이 추적
  useEffect(() => {
    const show = Keyboard.addListener("keyboardDidShow", (e) =>
      setKbHeight(e.endCoordinates.height)
    );
    const hide = Keyboard.addListener("keyboardDidHide", () => setKbHeight(0));
    return () => {
      show.remove();
      hide.remove();
    };
  }, []);

  const client = useMemo(() => (session ? makeClient(session.baseUrl, session.token) : null), [session]);

  function connectWs(runId: string, s: Session) {
    doneRef.current = false;
    const ws = new WebSocket(streamUrl(s.baseUrl, runId, 0, s.token));
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
        fetchDiff(s);
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
        connectWs(runId, s);
        return;
      }
      doneRef.current = true;
      setChat((prev) => ({ ...prev, running: false, error: prev.error ?? "연결이 끊겼습니다." }));
    };

    ws.onerror = () => {
      // onclose fires right after; nothing to log (never log token/url).
    };
  }

  async function fetchDiff(s: Session) {
    if (!path) return;
    try {
      const d: DiffSummary = await makeClient(s.baseUrl, s.token).diff(path);
      setDiff(d);
    } catch {
      // git summary is best-effort; ignore failures.
    }
  }

  async function handleSend() {
    if (!session || !client || !project) return;
    const text = prompt.trim();
    if (!text || chat.running) return;

    setSendError(null);
    setDiff(null);
    setPrompt("");
    reconnectedRef.current = false;

    const userMsg: ChatMsg = { role: "user", text };
    setChat((prev) => ({ ...initialChatState(), messages: [...prev.messages, userMsg] }));

    try {
      const { run_id } = await client.chat(project, text, plan);
      runIdRef.current = run_id;
      connectWs(run_id, session);
    } catch (e) {
      if (isUnauthorized(e)) {
        // Token revoked/invalid → drop it and send the user back to pairing.
        await clearSession();
        router.replace("/pair");
        return;
      }
      setSendError("전송 실패. 다시 시도해주세요.");
      setChat((prev) => ({ ...prev, running: false }));
    }
  }

  function handleCancel() {
    const runId = runIdRef.current;
    if (!session || !runId) return;
    // Fire-and-forget: the run's WS stream will still emit a terminal `done`
    // event once the process is killed, which drives the rest of the UI.
    makeClient(session.baseUrl, session.token)
      .cancel(runId)
      .catch(() => {});
  }

  if (!session) {
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
      <ScrollView
        ref={scrollRef}
        style={styles.list}
        contentContainerStyle={styles.listContent}
        onContentSizeChange={() => scrollRef.current?.scrollToEnd({ animated: true })}
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

      <View
        style={[
          styles.inputBar,
          { marginBottom: kbHeight, paddingBottom: kbHeight > 0 ? 8 : insets.bottom + 8 },
        ]}
      >
        <View style={styles.planRow}>
          <Text style={styles.planLabel}>plan</Text>
          <Switch value={plan} onValueChange={setPlan} disabled={running} />
        </View>
        <TextInput
          style={styles.input}
          multiline
          value={prompt}
          onChangeText={setPrompt}
          editable={!running}
          placeholder="메시지를 입력하세요"
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
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
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
    alignItems: "center",
  },
  planLabel: {
    fontSize: 11,
    color: "#666",
  },
  input: {
    flex: 1,
    borderWidth: StyleSheet.hairlineWidth,
    borderColor: "#ccc",
    borderRadius: 8,
    paddingHorizontal: 10,
    paddingVertical: 6,
    maxHeight: 120,
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
