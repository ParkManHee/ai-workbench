import { useEffect, useMemo, useRef, useState } from "react";
import {
  ActivityIndicator,
  Image,
  Pressable,
  ScrollView,
  StyleSheet,
  Switch,
  Text,
  TextInput,
  View,
} from "react-native";
import * as ImagePicker from "expo-image-picker";
import { KeyboardStickyView, useKeyboardState } from "react-native-keyboard-controller";
import { router, useLocalSearchParams, Stack } from "expo-router";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import { isUnauthorized, makeClient, streamUrl } from "../../src/lib/api";
import { initialChatState, reduceEvent, verdictLabel } from "../../src/lib/events";
import type { ChatMsg, ChatState, PC, TranscriptMsg, WsEvent } from "../../src/lib/types";
import Markdown from "react-native-markdown-display";
import { getPC, removePC } from "../../src/store/pcs";
import { useTheme, type Theme } from "../../src/lib/theme";

/** 자주 쓰는 지시 — 탭하면 입력창에 채워진다(바로 전송 아님). */
const PROMPT_PRESETS = [
  "이어서 진행해줘",
  "현재 상황 요약해줘",
  "테스트 돌려줘",
  "코드리뷰 해줘",
  "커밋해줘",
  "작업로그 갱신해줘",
];

/** 데몬 트랜스크립트 항목 → 화면 채팅 메시지. role은 자유 문자열이라 user 외엔 assistant로 취급. */
function toChatMsg(m: TranscriptMsg): ChatMsg {
  return { role: m.role === "user" ? "user" : "assistant", text: m.text, tools: m.tools, toolDetails: m.tool_details, options: m.options };
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
  const t = useTheme();
  const styles = useMemo(() => makeStyles(t), [t]);
  // 어시스턴트 말풍선의 마크다운 렌더 스타일(테마 연동) — **, ``` 등이 원문으로 노출되지 않게 한다
  const mdStyles = useMemo(
    () => ({
      body: { color: t.text, fontSize: 15 },
      paragraph: { marginTop: 0, marginBottom: 6 },
      heading1: { color: t.text, fontSize: 19, fontWeight: "700" as const, marginVertical: 4 },
      heading2: { color: t.text, fontSize: 17, fontWeight: "700" as const, marginVertical: 4 },
      heading3: { color: t.text, fontSize: 15, fontWeight: "700" as const, marginVertical: 3 },
      strong: { fontWeight: "700" as const },
      link: { color: t.accent },
      bullet_list_icon: { color: t.text },
      ordered_list_icon: { color: t.text },
      code_inline: { backgroundColor: t.mono, color: t.text, fontFamily: "monospace", borderRadius: 4 },
      fence: { backgroundColor: t.mono, borderColor: t.border, borderRadius: 6, padding: 8, marginVertical: 4 },
      code_block: { backgroundColor: t.mono, borderColor: t.border, color: t.text, fontFamily: "monospace", fontSize: 11 },
      blockquote: { backgroundColor: t.box, borderLeftColor: t.border, marginVertical: 4 },
      hr: { backgroundColor: t.border },
      table: { borderColor: t.border },
      th: { color: t.text, borderColor: t.border },
      td: { color: t.text, borderColor: t.border },
      tr: { borderColor: t.border },
    }),
    [t]
  );
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
  // 첨부 이미지(전송 전): 갤러리에서 선택, 업로드는 전송 시점에 수행(uri는 썸네일 표시용)
  const [images, setImages] = useState<{ uri: string; ext: string; base64: string }[]>([]);
  const [uploading, setUploading] = useState(false);
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
  // 연속 재접속 한도 — 데이터 수신 시 리셋되므로 긴 실행 중 산발적 순단은 계속 복구된다.
  const MAX_RECONNECTS = 5;
  const reconnectsRef = useRef(0);
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

  // 실행 종료(정상/실패/스트림 포기) 후: 서버 트랜스크립트 기준으로 대화를 다시 맞추고
  // 실시간 폴을 재개한다 — handleSend가 stopPoll()한 채 방치되면 이후 PC 쪽 대화가
  // 하나도 안 올라오는 "얼어붙은 방"이 된다(이번 버그). 서버 진실로 교체하므로
  // WS로 이미 그린 내용과 중복되지 않는다.
  async function reloadTailAndPoll(p: PC) {
    if (!session) return;
    try {
      const res = await makeClient(p.baseUrl, p.token).transcriptTail(project, session);
      nextLineRef.current = res.next;
      prevRef.current = res.prev;
      setChat((prev) => ({ ...prev, messages: res.messages.map(toChatMsg) }));
      if (res.active) startPoll(p);
    } catch {
      // best-effort: 다음 진입 시 초기 로드가 복구한다
    }
  }

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
      reconnectsRef.current = 0; // 데이터가 흐르면 재접속 카운터 리셋(긴 실행 중 여러 번 끊겨도 복구)
      setChat((prev) => reduceEvent(prev, ev));
      if (ev.kind === "done") {
        doneRef.current = true;
        fetchDiff(p);
        ws.close();
        reloadTailAndPoll(p);
      } else if (ev.kind === "error") {
        doneRef.current = true;
        ws.close();
        reloadTailAndPoll(p);
      }
    };

    ws.onclose = () => {
      if (wsRef.current !== ws) return; // superseded by a newer run/reconnect
      if (doneRef.current) return;
      if (reconnectsRef.current < MAX_RECONNECTS) {
        // 절전/네트워크 순단으로 자주 끊기므로 백오프 재접속. offset=0 replay 방식이라
        // 스트리밍 중이던 assistant 말풍선을 지워 재생 토큰이 이중으로 붙지 않게 한다.
        reconnectsRef.current += 1;
        const delay = Math.min(1000 * reconnectsRef.current, 5000);
        setChat((prev) => {
          const msgs = [...prev.messages];
          if (msgs.at(-1)?.role === "assistant") msgs.pop();
          return { ...initialChatState(), messages: msgs };
        });
        setTimeout(() => {
          if (wsRef.current === ws && !doneRef.current) connectWs(runId, p);
        }, delay);
        return;
      }
      // 스트림 포기: 실행은 계속될 수 있으므로 트랜스크립트 폴로 전환해 진행 내용을 보여준다
      doneRef.current = true;
      setChat((prev) => ({ ...prev, running: false, error: prev.error ?? "스트림이 끊겨 기록 보기로 전환했습니다." }));
      reloadTailAndPoll(p);
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
      setOpenDiffs({}); // 새 요약 기준으로 펼침 상태 초기화
    } catch {
      // git summary is best-effort; ignore failures.
    }
  }

  // 파일 행 탭 → 해당 파일 unified diff 펼침/접힘
  const [openDiffs, setOpenDiffs] = useState<Record<string, string>>({});
  async function toggleFileDiff(file: string) {
    if (openDiffs[file] !== undefined) {
      setOpenDiffs((prev) => {
        const next = { ...prev };
        delete next[file];
        return next;
      });
      return;
    }
    if (!pc || !path) return;
    try {
      const res = await makeClient(pc.baseUrl, pc.token).diffFile(path, file);
      setOpenDiffs((prev) => ({ ...prev, [file]: res.diff.trim() ? res.diff : "(표시할 diff 없음)" }));
    } catch {
      setOpenDiffs((prev) => ({ ...prev, [file]: "(diff 조회 실패)" }));
    }
  }

  async function pickImages() {
    const res = await ImagePicker.launchImageLibraryAsync({
      mediaTypes: ["images"],
      allowsMultipleSelection: true,
      selectionLimit: 3,
      quality: 0.8,
      base64: true, // 업로드는 base64 → bytes 경로(expo 전역 fetch가 uri FormData 파트 미지원)
    });
    if (res.canceled || !res.assets) return;
    const picked = res.assets
      .filter((a) => !!a.base64)
      .map((a) => {
        // 서버 화이트리스트(jpg/jpeg/png/webp)에 맞춰 확장자 결정 — 모르면 jpg
        let ext = (a.mimeType?.split("/")[1] ?? a.fileName?.split(".").pop() ?? "jpg").toLowerCase();
        if (!["jpg", "jpeg", "png", "webp"].includes(ext)) ext = "jpg";
        return { uri: a.uri, ext, base64: a.base64! };
      });
    setImages((prev) => [...prev, ...picked].slice(0, 3));
  }

  async function handleSend(overrideText?: string) {
    if (!pc || !client || !project) return;
    const text = (overrideText ?? prompt).trim();
    if ((!text && images.length === 0) || uploading) return; // 실행 중에도 전송 허용(서버가 큐잉)

    setSendError(null);
    // 이전 버전 코드로 선택된 이미지(base64 없음)가 핫리로드로 상태에 남아있을 수 있다
    if (images.some((im) => !im.base64)) {
      setSendError("이미지 정보가 유효하지 않습니다. 첨부를 ✕로 지우고 다시 선택해주세요.");
      return;
    }
    // 이미지 먼저 업로드 — 실패하면 입력을 유지한 채 전송 중단(부분 업로드로 실행하지 않음)
    const paths: string[] = [];
    if (images.length > 0) {
      setUploading(true);
      try {
        for (const im of images) paths.push((await client.upload(im.base64, im.ext)).path);
      } catch (e) {
        if (isUnauthorized(e)) {
          await removePC(pc.id);
          router.replace("/");
          return;
        }
        console.log("[awb] upload failed:", e); // adb logcat(ReactNativeJS)에서 원인 확인용
        setSendError(`이미지 업로드 실패: ${e instanceof Error ? e.message : String(e)}`);
        return;
      } finally {
        setUploading(false);
      }
    }

    // 에이전트는 Read 도구로 Mac에 저장된 첨부 이미지를 본다
    const fullPrompt = paths.length
      ? `${text}\n\n${paths.map((p) => `[첨부 이미지: ${p} — Read 도구로 확인]`).join("\n")}`
      : text;
    const shown = paths.length ? `${text}${text ? "\n" : ""}🖼 이미지 ${paths.length}장` : text;
    const userMsg: ChatMsg = { role: "user", text: shown };

    try {
      const res = await client.chat(project, fullPrompt, plan, session);
      setPrompt("");
      setImages([]);
      if (res.queued) {
        // 실행 중 → 서버 큐에 적재됨. 현재 뷰(스트림/폴)는 그대로 두고 안내만 붙인다.
        setChat((prev) => ({
          ...prev,
          messages: [
            ...prev.messages,
            userMsg,
            { role: "assistant", text: `⏳ 실행 중이라 대기열에 등록했습니다 (순번 ${res.position ?? 1}). 현재 턴이 끝나면 자동으로 전달됩니다.` },
          ],
        }));
        return;
      }
      stopPoll(); // the user's own run now drives the live view
      setDiff(null);
      reconnectsRef.current = 0;
      setChat((prev) => ({ ...initialChatState(), messages: [...prev.messages, userMsg] }));
      runIdRef.current = res.run_id!;
      connectWs(res.run_id!, pc);
    } catch (e) {
      if (isUnauthorized(e)) {
        // Token revoked/invalid → drop this PC and send the user back to the PC list.
        await removePC(pc.id);
        router.replace("/");
        return;
      }
      setSendError("전송 실패. 다시 시도해주세요.");
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
                m.role === "user" ? (
                  <Text style={styles.userText}>{m.text}</Text>
                ) : (
                  <Markdown style={mdStyles}>{m.text}</Markdown>
                )
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
              <View key={i}>
                <Pressable onPress={() => toggleFileDiff(e.path)}>
                  <Text style={styles.diffEntry}>
                    {openDiffs[e.path] !== undefined ? "▼" : "▶"} {e.status} {e.path}
                  </Text>
                </Pressable>
                {openDiffs[e.path] !== undefined ? (
                  <Text style={styles.diffText} selectable>
                    {openDiffs[e.path]}
                  </Text>
                ) : null}
              </View>
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

      <KeyboardStickyView>
        {/* 마지막 메시지가 선택지 질문(AskUserQuestion)이면 탭 한 번으로 답하는 버튼 */}
        {(() => {
          const last = chat.messages.at(-1);
          const opts = !chat.running && last?.role === "assistant" ? last.options ?? [] : [];
          if (opts.length === 0) return null;
          return (
            <View style={styles.optionRow}>
              {opts.map((o, i) => (
                <Pressable key={i} style={styles.optionChip} onPress={() => handleSend(o)}>
                  <Text style={styles.optionChipText}>{o}</Text>
                </Pressable>
              ))}
            </View>
          );
        })()}
        {/* 프리셋: 입력이 비어 있을 때만 — 탭하면 입력창에 채워져 수정 후 전송 */}
        {!prompt && !chat.running && images.length === 0 ? (
          <View style={styles.optionRow}>
            {PROMPT_PRESETS.map((p, i) => (
              <Pressable key={i} style={styles.presetChip} onPress={() => setPrompt(p)}>
                <Text style={styles.presetChipText}>{p}</Text>
              </Pressable>
            ))}
          </View>
        ) : null}
        {/* 에러는 키보드 위(입력바와 함께)로 — 리스트 아래에 두면 키보드에 가려 안 보인다 */}
        {sendError ? <Text style={styles.errorText}>{sendError}</Text> : null}
        {images.length > 0 ? (
          <View style={styles.attachRow}>
            {images.map((im, i) => (
              <View key={i} style={styles.thumbWrap}>
                <Image source={{ uri: im.uri }} style={styles.thumb} />
                <Pressable
                  style={styles.thumbRemove}
                  onPress={() => setImages((prev) => prev.filter((_, j) => j !== i))}
                >
                  <Text style={styles.thumbRemoveText}>✕</Text>
                </Pressable>
              </View>
            ))}
            {uploading ? <ActivityIndicator style={{ marginLeft: 4 }} /> : null}
          </View>
        ) : null}
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
        <Pressable style={styles.attachButton} onPress={pickImages} disabled={uploading || images.length >= 3}>
          <Text style={styles.attachButtonText}>🖼</Text>
        </Pressable>
        <TextInput
          style={styles.input}
          multiline
          value={prompt}
          onChangeText={setPrompt}
          editable={!uploading}
          placeholder="메시지를 입력하세요"
          placeholderTextColor={t.placeholder}
        />
        <Pressable
          style={styles.sendButton}
          onPress={() => handleSend()}
          disabled={(!prompt.trim() && images.length === 0) || uploading}
        >
          <Text style={styles.buttonText}>{uploading ? "업로드…" : running ? "큐잉" : "전송"}</Text>
        </Pressable>
        {running ? (
          <Pressable style={styles.cancelButton} onPress={handleCancel}>
            <Text style={styles.buttonText}>취소</Text>
          </Pressable>
        ) : null}
        </View>
      </KeyboardStickyView>
    </View>
  );
}

const makeStyles = (t: Theme) => StyleSheet.create({
  container: {
    flex: 1,
  },
  resumeBar: {
    paddingHorizontal: 12,
    paddingVertical: 6,
    backgroundColor: t.box,
    borderBottomWidth: StyleSheet.hairlineWidth,
    borderBottomColor: t.border,
    gap: 2,
  },
  resumeLabel: {
    fontSize: 11,
    color: t.subtext,
  },
  resumeCmd: {
    fontSize: 12,
    fontFamily: "monospace",
    color: t.text,
  },
  listWrap: {
    flex: 1,
  },
  list: {
    flex: 1,
  },
  optionRow: {
    flexDirection: "row",
    flexWrap: "wrap",
    gap: 6,
    paddingHorizontal: 10,
    paddingVertical: 6,
    borderTopWidth: StyleSheet.hairlineWidth,
    borderTopColor: t.border,
    backgroundColor: t.box,
  },
  optionChip: {
    backgroundColor: t.chip,
    borderColor: t.accent,
    borderWidth: StyleSheet.hairlineWidth,
    borderRadius: 14,
    paddingHorizontal: 12,
    paddingVertical: 6,
  },
  optionChipText: {
    color: t.accent,
    fontSize: 13,
    fontWeight: "600",
  },
  presetChip: {
    backgroundColor: t.chip,
    borderColor: t.border,
    borderWidth: StyleSheet.hairlineWidth,
    borderRadius: 14,
    paddingHorizontal: 11,
    paddingVertical: 5,
  },
  presetChipText: {
    color: t.chipText,
    fontSize: 12,
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
    backgroundColor: t.bubbleUser,
    borderRadius: 12,
    padding: 10,
    marginVertical: 4,
    maxWidth: "85%",
  },
  assistantBubble: {
    alignSelf: "flex-start",
    backgroundColor: t.bubbleBot,
    borderRadius: 12,
    padding: 10,
    marginVertical: 4,
    maxWidth: "85%",
  },
  userText: {
    fontSize: 15,
    color: t.text,
  },
  assistantText: {
    fontSize: 15,
    color: t.text,
  },
  toolOnlyRow: {
    alignSelf: "flex-start",
    maxWidth: "85%",
    marginVertical: 2,
  },
  toolButton: {
    alignSelf: "flex-start",
    backgroundColor: t.chip,
    borderRadius: 8,
    paddingHorizontal: 8,
    paddingVertical: 3,
    marginBottom: 4,
  },
  toolButtonText: {
    fontSize: 11,
    color: t.chipText,
  },
  toolDetail: {
    fontSize: 11,
    color: t.subtext,
    fontFamily: "monospace",
    backgroundColor: t.mono,
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
    color: t.text,
  },
  changedFiles: {
    fontSize: 12,
    color: t.subtext,
  },
  errorText: {
    color: "#c0392b",
    textAlign: "center",
    paddingVertical: 4,
  },
  diffBox: {
    backgroundColor: t.box,
    borderRadius: 8,
    padding: 10,
    marginTop: 4,
  },
  diffTitle: {
    fontWeight: "600",
    marginBottom: 4,
    color: t.text,
  },
  diffEntry: {
    fontSize: 12,
    color: t.text,
    paddingVertical: 2,
  },
  diffText: {
    fontSize: 10,
    fontFamily: "monospace",
    color: t.text,
    backgroundColor: t.mono,
    borderRadius: 6,
    padding: 6,
    marginVertical: 2,
  },
  inputBar: {
    flexDirection: "row",
    alignItems: "flex-end",
    padding: 8,
    gap: 8,
    borderTopWidth: StyleSheet.hairlineWidth,
    borderTopColor: t.border,
  },
  attachRow: {
    flexDirection: "row",
    alignItems: "center",
    gap: 8,
    paddingHorizontal: 12,
    paddingVertical: 8,
    borderTopWidth: StyleSheet.hairlineWidth,
    borderTopColor: t.border,
    backgroundColor: t.box,
  },
  thumbWrap: {
    position: "relative",
  },
  thumb: {
    width: 56,
    height: 56,
    borderRadius: 8,
    backgroundColor: t.chip,
  },
  thumbRemove: {
    position: "absolute",
    top: -6,
    right: -6,
    width: 20,
    height: 20,
    borderRadius: 10,
    backgroundColor: "rgba(0,0,0,0.65)",
    alignItems: "center",
    justifyContent: "center",
  },
  thumbRemoveText: {
    color: "white",
    fontSize: 11,
  },
  attachButton: {
    height: 40,
    justifyContent: "center",
    paddingHorizontal: 2,
  },
  attachButtonText: {
    fontSize: 20,
    color: t.text,
  },
  planRow: {
    flexDirection: "row",
    alignItems: "center",
    gap: 4,
    height: 40,
  },
  planLabel: {
    fontSize: 13,
    color: t.text,
  },
  input: {
    flex: 1,
    borderWidth: StyleSheet.hairlineWidth,
    borderColor: "#ccc",
    borderRadius: 8,
    paddingHorizontal: 10,
    paddingVertical: 6,
    maxHeight: 120,
    color: t.inputText,
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
