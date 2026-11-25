import { useEffect, useMemo, useRef, useState } from "react";
import {
  ActivityIndicator,
  Alert,
  AppState,
  Image,
  Platform,
  Pressable,
  ScrollView,
  Share,
  StyleSheet,
  Switch,
  Text,
  TextInput,
  ToastAndroid,
  View,
} from "react-native";
import * as Clipboard from "expo-clipboard";
import * as ImagePicker from "expo-image-picker";
import { ExpoSpeechRecognitionModule, useSpeechRecognitionEvent } from "expo-speech-recognition";
import { KeyboardStickyView, useKeyboardState } from "react-native-keyboard-controller";
import { router, useLocalSearchParams, Stack } from "expo-router";
import * as WebBrowser from "expo-web-browser";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import { isUnauthorized, makeClient, streamUrl } from "../../src/lib/api";
import { changedFilesLabel, initialChatState, reduceEvent, verdictLabel } from "../../src/lib/events";
import { rewriteBangCommand } from "../../src/lib/bang";
import type { ChatMsg, ChatState, PC, RateLimit, ResumeReservation, TranscriptMsg, WsEvent } from "../../src/lib/types";
import Markdown from "react-native-markdown-display";
import { getPC, removePC } from "../../src/store/pcs";
import { font, radius, space, TOUCH, useTheme, type Theme } from "../../src/lib/theme";
import { Button, Chip, Icon, IconButton } from "../../src/components/ui";
import { ChoiceSheet, type ChoiceSheetState } from "../../src/components/ChoiceSheet";
import { TextSelectModal } from "../../src/components/TextSelectModal";
import { GitSheet } from "../../src/components/GitSheet";

/** 자주 쓰는 지시 — 탭하면 입력창에 채워진다(바로 전송 아님). */
const PROMPT_PRESETS = [
  "이어서 진행해줘",
  "현재 상황 요약해줘",
  "테스트 돌려줘",
  "코드리뷰 해줘",
  "커밋해줘",
  "작업로그 갱신해줘",
  "/compact",
];

/** epoch초 → 현지 HH:mm */
function hhmm(sec: number) {
  const d = new Date(sec * 1000);
  return `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
}

/** 데몬 트랜스크립트 항목 → 화면 채팅 메시지. role은 자유 문자열이라 user 외엔 assistant로 취급. */
function toChatMsg(m: TranscriptMsg): ChatMsg {
  return { role: m.role === "user" ? "user" : "assistant", text: m.text, tools: m.tools, toolDetails: m.tool_details, options: m.options, todos: m.todos };
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
  /** 이 요약을 만든 턴(run) — 없으면 작업트리 전체. 파일 diff 도 같은 기준으로 연다 */
  run?: string;
}

export default function Chat() {
  const t = useTheme();
  const styles = useMemo(() => makeStyles(t), [t]);
  // 어시스턴트 말풍선의 마크다운 렌더 스타일(테마 연동) — **, ``` 등이 원문으로 노출되지 않게 한다
  const mdStyles = useMemo(
    () => ({
      body: { color: t.text, fontSize: 15, lineHeight: 22 },
      paragraph: { marginTop: 0, marginBottom: 6 },
      heading1: { color: t.text, fontSize: 19, fontWeight: "700" as const, marginVertical: 4 },
      heading2: { color: t.text, fontSize: 17, fontWeight: "700" as const, marginVertical: 4 },
      heading3: { color: t.text, fontSize: 15, fontWeight: "700" as const, marginVertical: 3 },
      strong: { fontWeight: "700" as const },
      link: { color: t.accent },
      bullet_list_icon: { color: t.text },
      ordered_list_icon: { color: t.text },
      code_inline: { backgroundColor: t.code, color: t.text, fontFamily: "monospace", borderRadius: 4 },
      fence: { backgroundColor: t.code, borderColor: t.border, color: t.text, borderRadius: radius.sm, padding: 8, marginVertical: 4 },
      code_block: { backgroundColor: t.code, borderColor: t.border, color: t.text, fontFamily: "monospace", fontSize: 11 },
      blockquote: { backgroundColor: t.surfaceAlt, borderLeftColor: t.border, marginVertical: 4 },
      hr: { backgroundColor: t.border },
      table: { borderColor: t.border },
      th: { color: t.text, borderColor: t.border },
      td: { color: t.text, borderColor: t.border },
      tr: { borderColor: t.border },
    }),
    [t]
  );
  const { project, pc: pcId, path, session, fresh } = useLocalSearchParams<{
    project: string;
    pc: string;
    path: string;
    session?: string;
    fresh?: string;
  }>();
  // "+ 새 대화" 로 열었으면 첫 전송만 새 세션 요청 — 없으면 서버가 저장된 마지막 세션을 잇는다
  const newSessionRef = useRef(fresh === "1");
  // 실제로 이어 쓰는 세션 — 터미널에 열린 세션을 고르면 서버가 갈라(fork) 새 ID 로 쓰고 done 으로 알려준다
  const sidRef = useRef<string | undefined>(session);
  const [sid, setSid] = useState<string | undefined>(session);
  const insets = useSafeAreaInsets();
  // 키보드가 열리면 입력바가 키보드 위로 올라가므로, 하단 내비바 안전영역 패딩을
  // 빼서 입력바와 키보드 사이 흰 공백을 없앤다(닫혀 있을 때만 안전영역 적용).
  const kbVisible = useKeyboardState((s) => s.isVisible);
  // 키보드가 열리면 입력바가 리스트 위로 겹쳐 올라오므로, 가려지는 만큼 하단 패딩을 준다.
  const kbHeight = useKeyboardState((s) => s.height);
  const kbPad = kbVisible ? kbHeight : 0;
  // undefined = not checked yet, null = checked and no PC found (redirecting)
  const [pc, setPc] = useState<PC | null | undefined>(undefined);
  const pcRef = useRef<PC | null>(null);
  pcRef.current = pc ?? null;
  // initialChatState() has running:true by design (it's the state reset when a
  // run starts, per reduceEvent's tests); the idle screen before any send
  // should not show a "cancel"/disabled input, so start with running:false.
  const [chat, setChat] = useState<ChatState>(() => ({ ...initialChatState(), running: false }));
  const [prompt, setPrompt] = useState("");
  const [plan, setPlan] = useState(false);
  // 권한 모드(claude --permission-mode) — 전체 외에는 claude 가 묻는 것을 폰으로 받는다
  const [mode, setMode] = useState("auto");
  // plan 실행 완료 후 "이 계획대로 진행" 원탭 칩
  const [showPlanApprove, setShowPlanApprove] = useState(false);
  const planAtSendRef = useRef(false);
  // 음성 입력(STT): 받아쓰기 시작 시점의 기존 입력을 보존하고 인식 결과를 이어붙인다
  const [listening, setListening] = useState(false);
  const dictationBaseRef = useRef("");
  // 모델 오버라이드("" = 기본): 긴 작업 전 상위/하위 모델 전환
  const [model, setModel] = useState("");
  const [effort, setEffort] = useState("");
  const [sheet, setSheet] = useState<ChoiceSheetState | null>(null);
  const [selectText, setSelectText] = useState<string | null>(null);
  const [todoOpen, setTodoOpen] = useState(false);
  const [perms, setPerms] = useState<{ id: string; tool_name: string; summary: string }[]>([]);
  const [denyReasons, setDenyReasons] = useState<Record<string, string>>({});
  const [diff, setDiff] = useState<DiffSummary | null>(null);
  // 변경 파일 목록은 기본 접힘 — 펼쳐 두면 파일이 많을 때 답변을 화면 위로 밀어낸다
  const [diffOpen, setDiffOpen] = useState(false);
  // 마지막 run 기준 "이번 턴" / 작업트리 "전체". 턴 diff 는 끝 스냅샷이 찍힌 뒤(실행 종료 후)에만 있다
  const diffScopeRef = useRef<"turn" | "all">("turn");
  const lastRunRef = useRef<string | null>(null);
  const [gitOpen, setGitOpen] = useState(false);
  // 응답 대기 중 재탭 차단 — state 는 다음 렌더까지 반영되지 않아 ref 로 즉시 막는다
  const [sending, setSending] = useState(false);
  const sendingRef = useRef(false);
  // 프리셋 칩은 입력칸 포커스 중에만 — 상시 노출하면 대화창을 가린다
  const [inputFocused, setInputFocused] = useState(false);
  // 대화창을 누르거나 끌면 입력칸만 남기고 접는다 — 컨트롤·프리셋·이어받기는 입력칸을 눌러야 편다
  const [controlsOpen, setControlsOpen] = useState(false);
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
  const bgDetachedRef = useRef(false);
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

  // 백그라운드에선 WS 를 닫는다 — 데몬은 WS 가 붙어 있으면 "보는 중"으로 보고 진행·완료 푸시를 건너뛴다.
  // 돌아오면 같은 run 에 offset=0 으로 재부착(끊김 재접속과 같은 경로)하고, 그새 끝났으면 서버 기록으로 맞춘다.
  useEffect(() => {
    const sub = AppState.addEventListener("change", async (st) => {
      if (st === "background") {
        const ws = wsRef.current;
        if (!ws) return;
        wsRef.current = null; // onclose 의 superseded 가드가 재접속·오류 표시를 막는다
        ws.close();
        bgDetachedRef.current = true;
      } else if (st === "active" && bgDetachedRef.current) {
        bgDetachedRef.current = false;
        const p = pcRef.current;
        if (!p || doneRef.current) return;
        let runId = runIdRef.current;
        try {
          const ar = await makeClient(p.baseUrl, p.token).activeRun(project);
          runId = ar.run_id ?? null;
          // 백그라운드 중 끝난 fork 실행은 done 을 못 받았다 — 서버가 기록한 갈라진 세션으로 옮긴다
          if (!runId && ar.session && ar.forked_from === sidRef.current) {
            sidRef.current = ar.session;
            setSid(ar.session);
          }
        } catch {
          // 조회 실패 시 기억한 run 으로 시도 — 끝났으면 unknown run → 기존 스트림 포기 경로가 기록으로 전환
        }
        if (!runId) {
          doneRef.current = true;
          setChat((prev) => ({ ...prev, running: false }));
          reloadTailAndPoll(p);
          return;
        }
        reconnectsRef.current = 0;
        setChat((prev) => {
          const msgs = [...prev.messages];
          if (msgs.at(-1)?.role === "assistant") msgs.pop(); // offset=0 재생이라 이중으로 붙지 않게
          return { ...initialChatState(), messages: msgs };
        });
        connectWs(runId, p);
      }
    });
    return () => sub.remove();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [project]);

  // 키보드가 열리면 최신 메시지가 입력바에 가려지지 않게 맨아래로 스크롤.
  useEffect(() => {
    if (kbVisible) {
      const t = setTimeout(() => scrollRef.current?.scrollToEnd({ animated: true }), 50);
      return () => clearTimeout(t);
    }
  }, [kbVisible]);

  // 사용량 한도·이어가기 예약 — 한도는 run 로그에서만 바뀌므로 진입 시와 실행이 끝날 때 갱신
  const [rateLimit, setRateLimit] = useState<RateLimit | null>(null);
  const [reservation, setReservation] = useState<ResumeReservation | null>(null);
  useEffect(() => {
    if (!pc || chat.running) return;
    const c = makeClient(pc.baseUrl, pc.token);
    Promise.all([c.info(), c.resumeAtReset(project)])
      .then(([info, r]) => {
        setRateLimit(info.rate_limit ?? null);
        setReservation(r);
      })
      .catch(() => {});
  }, [pc, chat.running, project]);

  async function reserveResume(rl: RateLimit) {
    const sessionId = rl.session_id ?? sidRef.current;
    if (!pc || !rl.resets_at || !sessionId) return;
    const r: ResumeReservation = { session_id: sessionId, resets_at: rl.resets_at, mode, ...(model ? { model } : {}), ...(effort ? { effort } : {}) };
    try {
      await makeClient(pc.baseUrl, pc.token).setResumeAtReset(project, r);
      setReservation(r);
    } catch (e) {
      Alert.alert("예약 실패", String(e));
    }
  }

  async function cancelResume() {
    if (!pc) return;
    try {
      await makeClient(pc.baseUrl, pc.token).cancelResumeAtReset(project);
      setReservation(null);
    } catch (e) {
      Alert.alert("취소 실패", String(e));
    }
  }

  // 진행 중 실행(다른 기기/이전 세션에서 시작)에 attach — 취소 버튼이 동작하게 한다.
  async function attachActiveRun(p: PC) {
    try {
      const r = await makeClient(p.baseUrl, p.token).activeRun(project);
      if (r.run_id && doneRef.current) {
        runIdRef.current = r.run_id;
        lastRunRef.current = r.run_id;
        setChat((prev) => ({ ...prev, running: true }));
      }
    } catch {
      // best-effort
    }
  }

  // 실행 중 diff 미리보기 — 완료를 기다리지 않고 10초 간격으로 변경 요약 갱신
  useEffect(() => {
    if (!chat.running || !pc) return;
    fetchDiff(pc);
    const iv = setInterval(() => fetchDiff(pc), 10000);
    return () => clearInterval(iv);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [chat.running, pc]);

  // 승인 대기 폴: 화면이 떠 있는 동안 이 프로젝트의 권한 요청을 2.5초 간격으로 확인
  useEffect(() => {
    if (!pc) return;
    const iv = setInterval(async () => {
      try {
        const r = await makeClient(pc.baseUrl, pc.token).permissionPending(project);
        setPerms(r.pending);
      } catch {
        // best-effort
      }
    }, 2500);
    return () => clearInterval(iv);
  }, [pc, project]);

  async function copyText(text: string) {
    try {
      await Clipboard.setStringAsync(text);
      if (Platform.OS === "android") ToastAndroid.show("복사됨", ToastAndroid.SHORT);
    } catch {
      // best-effort
    }
  }

  function bubbleActions(role: ChatMsg["role"], text: string) {
    setSheet({ title: role === "user" ? "내 메시지" : "답변", choices: [
      { label: "전체 복사하기", onPress: () => copyText(text) },
      // 시트가 닫히는 렌더가 먼저 커밋된 뒤 열어 Modal 두 개가 겹치지 않게 한다
      { label: "부분만 복사하기", onPress: () => setTimeout(() => setSelectText(text), 0) },
      { label: "입력창에 넣기", onPress: () => setPrompt(text) },
      { label: "공유하기", onPress: () => { Share.share({ message: text }).catch(() => {}); } },
    ] });
  }

  async function answerPermission(id: string, allow: boolean, forSession = false) {
    if (!pc) return;
    setPerms((prev) => prev.filter((p) => p.id !== id));
    const reason = allow ? undefined : denyReasons[id]?.trim() || undefined;
    try {
      await makeClient(pc.baseUrl, pc.token).permissionAnswer(id, allow, forSession, reason);
    } catch {
      // 실패 시 다음 폴에서 다시 나타난다
    }
  }

  useSpeechRecognitionEvent("result", (e) => {
    const t = e.results?.[0]?.transcript ?? "";
    setPrompt(`${dictationBaseRef.current}${dictationBaseRef.current && t ? " " : ""}${t}`);
  });
  useSpeechRecognitionEvent("end", () => setListening(false));
  useSpeechRecognitionEvent("error", () => setListening(false));

  function pickModel() {
    setSheet({ title: "모델", message: `현재: ${model || "기본"}`, choices: [
      { label: "기본(설정 따름)", onPress: () => setModel("") },
      { label: "Opus", onPress: () => setModel("opus") },
      { label: "Sonnet", onPress: () => setModel("sonnet") },
      { label: "Haiku", onPress: () => setModel("haiku") },
    ] });
  }

  function pickMode() {
    setSheet({ title: "권한 모드", message: `현재: ${MODE_LABELS[mode]}`, choices:
      Object.entries(MODE_LABELS).map(([m, label]) => ({ label, onPress: () => setMode(m) })),
    });
  }

  function pickEffort() {
    setSheet({ title: "Effort", message: `현재: ${effort || "기본"}`, choices: [
      { label: "기본(설정 따름)", onPress: () => setEffort("") },
      ...["low", "medium", "high", "xhigh", "max"].map((e) => ({ label: e, onPress: () => setEffort(e) })),
    ] });
  }

  async function toggleMic() {
    if (listening) {
      ExpoSpeechRecognitionModule.stop();
      return;
    }
    try {
      const perm = await ExpoSpeechRecognitionModule.requestPermissionsAsync();
      if (!perm.granted) return;
      dictationBaseRef.current = prompt.trim();
      setListening(true);
      ExpoSpeechRecognitionModule.start({ lang: "ko-KR", interimResults: true });
    } catch {
      setListening(false);
    }
  }

  // Mac에서 리슨 중인 dev 서버를 폰 브라우저로 열기(Tailscale 경유)
  async function openDevServerPreview() {
    if (!pc) return;
    try {
      const servers = await makeClient(pc.baseUrl, pc.token).devServers();
      const host = pc.baseUrl.replace(/^https?:\/\//, "").replace(/:\d+$/, "");
      const reachable = servers.filter((s) => s.reachable);
      if (reachable.length === 0) {
        const local = servers.filter((s) => !s.reachable).map((s) => `${s.process}:${s.port}`).join(", ");
        Alert.alert("프리뷰", local
          ? `열 수 있는 서버가 없습니다.\n로컬 전용(${local})은 dev 서버를 --host 0.0.0.0 으로 띄워야 폰에서 접근됩니다.`
          : "리슨 중인 dev 서버가 없습니다.");
        return;
      }
      setSheet({ title: "dev 서버 프리뷰", message: "열 서버를 선택하세요", choices: reachable.map((s) => ({
          label: `${s.process} :${s.port}`,
          // 외부 브라우저 대신 Custom Tab — 상단 X 로 닫고 앱으로 돌아온다(createTask:false 로 앱 태스크 안에 띄움)
          onPress: () => WebBrowser.openBrowserAsync(`http://${host}:${s.port}`, { showTitle: true, createTask: false }),
        })) });
    } catch {
      Alert.alert("프리뷰", "dev 서버 조회 실패");
    }
  }

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
    if (pollRef.current || !sidRef.current) return;
    pollRef.current = setInterval(async () => {
      if (!doneRef.current) return; // local run in flight; let its WS drive the UI
      if (pollBusyRef.current) return; // 이전 틱이 아직 진행 중(대용량/느린 네트워크) — 겹치면 같은 offset을 두 번 읽어 메시지가 중복된다
      pollBusyRef.current = true;
      try {
        const res = await makeClient(p.baseUrl, p.token).transcript(project, sidRef.current!, nextLineRef.current);
        nextLineRef.current = res.next; // 항상 전진(필터로 메시지가 비어도 재파싱 중복 방지)
        if (res.messages.length > 0) {
          setChat((prev) => ({ ...prev, messages: [...prev.messages, ...res.messages.map(toChatMsg)] }));
        }
        if (!res.active) {
          stopPoll();
          // attach 상태 해제(원격 실행 종료)
          if (doneRef.current) {
            runIdRef.current = null;
            setChat((prev) => (prev.running ? { ...prev, running: false } : prev));
          }
        }
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
    if (!pc || !sidRef.current || loadingOlderRef.current || prevRef.current == null) return;
    loadingOlderRef.current = true;
    setLoadingOlder(true);
    try {
      const res = await makeClient(pc.baseUrl, pc.token).transcriptBefore(project, sidRef.current, prevRef.current);
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
        if (res.active) {
          startPoll(pc);
          attachActiveRun(pc); // 진행 중 실행이 있으면 취소 가능하게 attach
        }
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
    if (!sidRef.current) return;
    try {
      const res = await makeClient(p.baseUrl, p.token).transcriptTail(project, sidRef.current);
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
        if (ev.session_id) {
          sidRef.current = ev.session_id;
          setSid(ev.session_id);
        }
        fetchDiff(p);
        ws.close();
        reloadTailAndPoll(p);
        if (planAtSendRef.current) setShowPlanApprove(true); // plan 결과 → 원탭 승인 제안
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
      const c = makeClient(p.baseUrl, p.token);
      const run = diffScopeRef.current === "turn" && doneRef.current ? lastRunRef.current ?? undefined : undefined;
      // 스냅샷이 없는 run(git 저장소 아님·이전 데몬)은 전체로 — 토글은 실제로 받은 쪽을 표시한다
      const d: DiffSummary = run ? await c.diff(path, run).then((x) => ({ ...x, run }), () => c.diff(path)) : await c.diff(path);
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
      const res = await makeClient(pc.baseUrl, pc.token).diffFile(path, file, diff?.run);
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

  async function handleSend(overrideText?: string, overridePlan?: boolean) {
    if (!pc || !client || !project) return;
    if (sendingRef.current) return;
    const planFlag = overridePlan ?? plan;
    const text = (overrideText ?? prompt).trim();
    if ((!text && images.length === 0) || uploading) return; // 실행 중에도 전송 허용(서버가 큐잉)

    setSendError(null);
    // 이전 버전 코드로 선택된 이미지(base64 없음)가 핫리로드로 상태에 남아있을 수 있다
    if (images.some((im) => !im.base64)) {
      setSendError("이미지 정보가 유효하지 않습니다. 첨부를 ✕로 지우고 다시 선택해주세요.");
      return;
    }
    sendingRef.current = true;
    setSending(true);
    // 탭 즉시 입력을 비워 반응을 보이고, 실패하면 복원한다(그 사이 새로 친 입력은 덮지 않음)
    const sentPrompt = prompt;
    const sentImages = images;
    const restoreInput = () => {
      setPrompt((cur) => (cur ? cur : sentPrompt));
      setImages((cur) => (cur.length ? cur : sentImages));
    };
    setPrompt("");
    setImages([]);
    try {
    // 이미지 먼저 업로드 — 실패하면 입력을 유지한 채 전송 중단(부분 업로드로 실행하지 않음)
    const paths: string[] = [];
    if (sentImages.length > 0) {
      setUploading(true);
      try {
        for (const im of sentImages) paths.push((await client.upload(im.base64, im.ext)).path);
      } catch (e) {
        if (isUnauthorized(e)) {
          await removePC(pc.id);
          router.replace("/");
          return;
        }
        console.log("[awb] upload failed:", e); // adb logcat(ReactNativeJS)에서 원인 확인용
        restoreInput();
        setSendError(`이미지 업로드 실패: ${e instanceof Error ? e.message : String(e)}`);
        return;
      } finally {
        setUploading(false);
      }
    }

    // 말풍선엔 사용자가 친 `!cmd` 원문, 서버엔 실행 지시로 변환한 프롬프트
    const sendText = rewriteBangCommand(text);
    // 에이전트는 Read 도구로 Mac에 저장된 첨부 이미지를 본다
    const fullPrompt = paths.length
      ? `${sendText}\n\n${paths.map((p) => `[첨부 이미지: ${p} — Read 도구로 확인]`).join("\n")}`
      : sendText;
    const shown = paths.length ? `${text}${text ? "\n" : ""}🖼 이미지 ${paths.length}장` : text;
    const userMsg: ChatMsg = { role: "user", text: shown };

    try {
      setShowPlanApprove(false);
      planAtSendRef.current = planFlag;
      const res = await client.chat(project, fullPrompt, planFlag, sidRef.current, mode, model || undefined, effort || undefined, newSessionRef.current);
      newSessionRef.current = false;
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
      setDiffOpen(false);
      reconnectsRef.current = 0;
      setChat((prev) => ({ ...initialChatState(), messages: [...prev.messages, userMsg] }));
      runIdRef.current = res.run_id!;
      lastRunRef.current = res.run_id!;
      connectWs(res.run_id!, pc);
    } catch (e) {
      if (isUnauthorized(e)) {
        // Token revoked/invalid → drop this PC and send the user back to the PC list.
        await removePC(pc.id);
        router.replace("/");
        return;
      }
      restoreInput();
      setSendError("전송 실패. 다시 시도해주세요.");
    }
    } finally {
      sendingRef.current = false;
      setSending(false);
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
      {sid && path && controlsOpen ? (
        <Pressable style={styles.resumeBar} onLongPress={() => copyText(`cd ${path} && claude --resume ${sid}`)}>
          <View style={styles.resumeTextCol}>
            <Text style={styles.resumeLabel}>PC에서 이어받기:</Text>
            <Text style={styles.resumeCmd}>{`cd ${path} && claude --resume ${sid}`}</Text>
          </View>
          <IconButton name="content-copy" label="이어받기 명령 복사" color={t.accent} onPress={() => copyText(`cd ${path} && claude --resume ${sid}`)} />
        </Pressable>
      ) : null}
      {(() => {
        const withTodos = [...chat.messages].reverse().find((m) => m.todos && m.todos.length > 0);
        const todos = withTodos?.todos ?? [];
        if (todos.length === 0) return null;
        const doneN = todos.filter((x) => x.startsWith("✔")).length;
        const cur = todos.find((x) => x.startsWith("▶"));
        return (
          <Pressable style={styles.todoBar} onPress={() => setTodoOpen((v) => !v)} accessibilityRole="button">
            <View style={styles.barRow}>
              <Icon name="checklist" size={18} color={t.accent} />
              <Text style={styles.todoTitle} numberOfLines={1}>
                진행 {doneN}/{todos.length}{cur ? ` · ${cur.slice(2)}` : ""}
              </Text>
              <Icon name={todoOpen ? "expand-less" : "expand-more"} size={20} color={t.subtext} />
            </View>
            {todoOpen ? todos.map((x, i) => (
              <Text key={i} style={styles.todoItem}>{x}</Text>
            )) : null}
          </Pressable>
        );
      })()}
      {(() => {
        // 리셋 시각이 지난 값은 낡은 상태 — 다음 run 이 새로 알려 줄 때까지 숨긴다
        const rl = rateLimit?.resets_at && rateLimit.resets_at > Date.now() / 1000 ? rateLimit : null;
        const limited = rl && rl.status !== "allowed" ? rl : null;
        const canReserve = !reservation && !running && limited?.status === "rejected" && limited.project === project;
        if (!limited && !reservation) return null;
        const limitColor = limited?.status === "rejected" ? t.danger : t.warning;
        return (
          <View style={styles.limitBar}>
            {limited ? (
              <View style={styles.barRow}>
                <Icon name={limited.status === "rejected" ? "block" : "warning-amber"} size={16} color={limitColor} />
                <Text style={[styles.limitText, { color: limitColor }]}>
                  {limited.status === "rejected" ? "사용량 한도 도달" : "사용량 한도 임박"} · 리셋 {hhmm(limited.resets_at!)}
                </Text>
              </View>
            ) : null}
            {reservation ? (
              <Chip icon="schedule" label={`예약됨 ${hhmm(reservation.resets_at + 60)} · 취소`} onPress={cancelResume} />
            ) : canReserve ? (
              <Chip icon="schedule" label="리셋 때 이어가기" onPress={() => reserveResume(limited!)} />
            ) : null}
          </View>
        );
      })()}
      <View style={styles.listWrap}>
      <ScrollView
        ref={scrollRef}
        style={styles.list}
        contentContainerStyle={[styles.listContent, kbPad > 0 ? { paddingBottom: kbPad + 12 } : null]}
        scrollEventThrottle={100}
        onTouchStart={() => setControlsOpen(false)}
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
        {loadingOlder ? <ActivityIndicator style={{ marginVertical: 8 }} color={t.accent} /> : null}
        {chat.messages.map((m, i) => {
          const hasTools = m.role === "assistant" && m.tools && m.tools.length > 0;
          const hasText = m.text.trim().length > 0;
          const isOpen = expandedTools.has(i);
          const details = m.toolDetails && m.toolDetails.length > 0 ? m.toolDetails : m.tools ?? [];
          const toolToggle = hasTools ? (
            <Pressable style={styles.toolButton} onPress={() => toggleTools(i)} hitSlop={10} accessibilityRole="button">
              <Icon name="build" size={14} color={t.subtext} />
              <Text style={styles.toolButtonText}>작업 {m.tools!.length}</Text>
              <Icon name={isOpen ? "expand-less" : "expand-more"} size={16} color={t.subtext} />
            </Pressable>
          ) : null;
          const toolList = hasTools && isOpen
            ? details.map((d, di) => (
                <Text key={di} style={styles.toolDetail} selectable>
                  {d}
                </Text>
              ))
            : null;
          // 툴만 있는 항목(답변 텍스트 없음): 말풍선 대신 작은 접힘 버튼만
          if (m.role === "assistant" && hasTools && !hasText) {
            return (
              <View key={i} style={styles.toolOnlyRow}>
                {toolToggle}
                {toolList}
              </View>
            );
          }
          return (
            <Pressable
              key={i}
              style={m.role === "user" ? styles.userBubble : styles.assistantBubble}
              onLongPress={hasText ? () => bubbleActions(m.role, m.text) : undefined}
            >
              {toolToggle}
              {toolList}
              {hasText ? (
                m.role === "user" ? (
                  <Text style={styles.userText}>{m.text}</Text>
                ) : (
                  <Markdown style={mdStyles}>{m.text}</Markdown>
                )
              ) : null}
            </Pressable>
          );
        })}

        {!chat.running && chat.verdict ? (
          <View style={styles.verdictRow}>
            <View style={styles.barRow}>
              <Icon
                name={chat.verdict === "failed" ? "error" : "check-circle"}
                size={18}
                color={chat.verdict === "failed" ? t.danger : t.success}
              />
              <Text style={styles.verdictText}>{verdictLabel(chat.verdict).replace(/^(✅|❌|⏳) /, "")}</Text>
            </View>
            <Text style={styles.changedFiles}>{changedFilesLabel(chat.changedFiles, diff)}</Text>
          </View>
        ) : null}

        {chat.error ? (
          <View style={styles.errorRow}>
            <Icon name="error-outline" size={16} color={t.danger} />
            <Text style={styles.errorText}>{chat.error}</Text>
          </View>
        ) : null}

        {diff ? (
          <View style={styles.diffBox}>
            <Pressable style={styles.diffHeader} onPress={() => setDiffOpen((v) => !v)} accessibilityRole="button">
              <Icon name="difference" size={18} color={t.subtext} />
              <Text style={styles.diffTitle}>
                git 변경 · {diff.run ? "이번 턴" : "전체"}: 파일 {diff.files}개 · +{diff.insertions} / -{diff.deletions}
              </Text>
              <Icon name={diffOpen ? "expand-less" : "expand-more"} size={20} color={t.subtext} />
            </Pressable>
            {diffOpen && lastRunRef.current && !chat.running ? (
              <View style={styles.chipRow}>
                {(["turn", "all"] as const).map((sc) => (
                  <Chip key={sc} label={sc === "turn" ? "이번 턴" : "전체"} selected={(diff.run ? "turn" : "all") === sc}
                    onPress={() => { diffScopeRef.current = sc; if (pc) fetchDiff(pc); }} />
                ))}
              </View>
            ) : null}
            {diffOpen ? diff.entries.map((e, i) => (
              <View key={i}>
                <Pressable style={styles.diffEntryRow} onPress={() => toggleFileDiff(e.path)} accessibilityRole="button">
                  <Icon name={openDiffs[e.path] !== undefined ? "expand-more" : "chevron-right"} size={18} color={t.subtext} />
                  <Text style={styles.diffStatus}>{e.status}</Text>
                  <Text style={styles.diffEntry} numberOfLines={2}>{e.path}</Text>
                </Pressable>
                {openDiffs[e.path] !== undefined ? (
                  <Text style={styles.diffText} selectable>
                    {openDiffs[e.path]}
                  </Text>
                ) : null}
              </View>
            )) : null}
          </View>
        ) : null}
      </ScrollView>
      {showJumpDown ? (
        <Pressable
          style={[styles.jumpDownButton, kbPad > 0 ? { bottom: 14 + kbPad } : null]}
          onPress={() => scrollRef.current?.scrollToEnd({ animated: true })}
          accessibilityRole="button"
          accessibilityLabel="맨 아래로"
        >
          <Icon name="arrow-downward" size={22} color={t.text} />
        </Pressable>
      ) : null}
      </View>

      <KeyboardStickyView>
        {/* 승인 모드 실행의 권한 요청 — 허용/거부 */}
        {perms.map((pm) => (
          <View key={pm.id} style={styles.permCard}>
            <View style={styles.barRow}>
              <Icon name="lock" size={18} color={t.warning} />
              <Text style={styles.permTitle}>권한 요청: {pm.tool_name}</Text>
            </View>
            <Text style={styles.permBody} numberOfLines={3}>{pm.summary}</Text>
            <View style={styles.permButtons}>
              <Button title="이번만" style={styles.permButton} onPress={() => answerPermission(pm.id, true)} />
              <Button title="세션 동안" variant="secondary" style={styles.permButton} onPress={() => answerPermission(pm.id, true, true)} />
              <Button title="거절" variant="danger" style={styles.permButton} onPress={() => answerPermission(pm.id, false)} />
            </View>
            <TextInput
              style={styles.permReason}
              value={denyReasons[pm.id] ?? ""}
              onChangeText={(v) => setDenyReasons((prev) => ({ ...prev, [pm.id]: v }))}
              placeholder="거절 사유(선택) — claude 에게 전달"
              placeholderTextColor={t.placeholder}
            />
          </View>
        ))}
        {/* 실행 종료 + 변경 있음 → 커밋·푸시·PR 시트 */}
        {!chat.running && diff && diff.files > 0 ? (
          <View style={styles.chipBar}>
            <Chip icon="commit" label="커밋·푸시·PR" selected onPress={() => setGitOpen(true)} />
          </View>
        ) : null}
        {/* plan 완료 → 원탭 실행 승인 */}
        {showPlanApprove && !chat.running ? (
          <View style={styles.chipBar}>
            <Chip icon="play-arrow" label="이 계획대로 진행" selected onPress={() => handleSend("좋아, 방금 세운 계획대로 진행해줘.", false)} />
            <Chip label="닫기" onPress={() => setShowPlanApprove(false)} />
          </View>
        ) : null}
        {/* 마지막 메시지가 선택지 질문(AskUserQuestion)이면 탭 한 번으로 답하는 버튼 */}
        {(() => {
          const last = chat.messages.at(-1);
          const opts = !chat.running && last?.role === "assistant" ? last.options ?? [] : [];
          if (opts.length === 0) return null;
          return (
            <View style={styles.chipBar}>
              {opts.map((o, i) => (
                <Chip key={i} label={o} selected onPress={() => handleSend(o)} />
              ))}
            </View>
          );
        })()}
        {/* 프리셋: 입력이 비어 있을 때만 — 탭하면 입력창에 채워져 수정 후 전송 */}
        {controlsOpen && inputFocused && !prompt && !chat.running && images.length === 0 ? (
          <View style={styles.chipBar}>
            {PROMPT_PRESETS.map((p, i) => (
              <Chip key={i} label={p} onPress={() => setPrompt(p)} />
            ))}
          </View>
        ) : null}
        {/* 에러는 키보드 위(입력바와 함께)로 — 리스트 아래에 두면 키보드에 가려 안 보인다 */}
        {sendError ? (
          <View style={[styles.errorRow, styles.sendError]}>
            <Icon name="error-outline" size={16} color={t.danger} />
            <Text style={styles.errorText}>{sendError}</Text>
          </View>
        ) : null}
        {images.length > 0 ? (
          <View style={styles.attachRow}>
            {images.map((im, i) => (
              <View key={i} style={styles.thumbWrap}>
                <Image source={{ uri: im.uri }} style={styles.thumb} />
                <Pressable
                  style={styles.thumbRemove}
                  hitSlop={14}
                  accessibilityRole="button"
                  accessibilityLabel="첨부 빼기"
                  onPress={() => setImages((prev) => prev.filter((_, j) => j !== i))}
                >
                  <Icon name="close" size={14} color="#ffffff" />
                </Pressable>
              </View>
            ))}
            {uploading ? <ActivityIndicator style={{ marginLeft: 4 }} color={t.accent} /> : null}
          </View>
        ) : null}
        <View style={[styles.inputBar, { paddingBottom: kbVisible ? 8 : insets.bottom + 8 }]}>
          {/* 컨트롤과 입력칸을 두 줄로 — 한 줄이면 컨트롤이 늘 때 입력칸 폭이 0 이 되고 전송 버튼이 화면 밖으로 밀린다 */}
          {controlsOpen || running ? (
          <View style={styles.controlRow}>
          {controlsOpen ? (
          <>
          <View style={styles.planRow}>
            <Text style={styles.planLabel}>plan</Text>
            <Switch
              value={plan}
              onValueChange={setPlan}
              disabled={running}
              trackColor={{ false: t.surfaceAlt, true: t.accent }}
              thumbColor={plan ? t.onAccent : t.subtext}
            />
          </View>
          {/* 모델·effort·권한은 각자 다른 시트를 여는 별개 버튼 — 칩으로 경계를 보이고, planRow 밖에 두어 좁은 폭에서 칩 단위로 줄바꿈 */}
          <Chip label={model || "모델"} selected={!!model} trailingIcon="arrow-drop-down" onPress={pickModel} />
          <Chip label={effort || "effort"} selected={!!effort} trailingIcon="arrow-drop-down" onPress={pickEffort} />
          <Chip trailingIcon="arrow-drop-down" onPress={pickMode} disabled={running}>
            <Text style={styles.chipText}>
              권한 <Text style={{ color: t.accent, fontWeight: font.weight.bold }}>{MODE_LABELS[mode]}</Text>
            </Text>
          </Chip>
          <View style={styles.iconRow}>
            <IconButton name="image" label="이미지 첨부" onPress={pickImages} disabled={uploading || images.length >= 3} />
            <IconButton name="public" label="dev 서버 프리뷰" onPress={openDevServerPreview} />
            <IconButton name={listening ? "mic-off" : "mic"} label={listening ? "받아쓰기 멈춤" : "받아쓰기"} color={listening ? t.danger : undefined} onPress={toggleMic} />
          </View>
          </>
          ) : null}
          {/* 취소는 컨트롤 줄에 — 입력 줄에 끼우면 전송 버튼이 밀려 그 자리에 취소가 와 오터치된다 */}
          {running ? (
            <Button title="취소" icon="stop" variant="danger" size="sm" onPress={handleCancel} />
          ) : null}
          </View>
          ) : null}
          <View style={styles.inputRow}>
            <TextInput
              style={styles.input}
              multiline
              value={prompt}
              onChangeText={setPrompt}
              onFocus={() => { setInputFocused(true); setControlsOpen(true); }}
              onPressIn={() => setControlsOpen(true)}
              onBlur={() => setInputFocused(false)}
              editable={!uploading}
              placeholder="메시지를 입력하세요"
              placeholderTextColor={t.placeholder}
            />
            {(() => {
              const disabled = (!prompt.trim() && images.length === 0) || uploading || sending;
              // 실행 중엔 보내면 대기열로 간다 — 아이콘만으론 구별이 안 돼 글자를 남긴다
              if (running && !uploading && !sending) {
                return <Button title="큐잉" size="sm" disabled={disabled} onPress={() => handleSend()} style={styles.queueButton} />;
              }
              return (
                <Pressable
                  style={({ pressed }) => [styles.sendButton, disabled ? { opacity: 0.4 } : pressed ? { opacity: 0.7 } : null]}
                  onPress={() => handleSend()}
                  disabled={disabled}
                  accessibilityRole="button"
                  accessibilityLabel={uploading ? "업로드 중" : sending ? "전송 중" : "전송"}
                  accessibilityState={{ disabled, busy: uploading || sending }}
                >
                  {uploading || sending ? <ActivityIndicator color={t.onAccent} /> : <Icon name="arrow-upward" size={24} color={t.onAccent} />}
                </Pressable>
              );
            })()}
          </View>
        </View>
      </KeyboardStickyView>
      <ChoiceSheet state={sheet} onClose={() => setSheet(null)} />
      <GitSheet pc={pc} project={project} visible={gitOpen} onClose={() => setGitOpen(false)} onCommitted={() => fetchDiff(pc)} />
      <TextSelectModal text={selectText} onClose={() => setSelectText(null)} onCopyAll={copyText} />
    </View>
  );
}

// 값은 서버 PERMISSION_MODES(claude --permission-mode)와 같아야 한다
const MODE_LABELS: Record<string, string> = { default: "감독", acceptEdits: "편집 자동", auto: "Auto", bypassPermissions: "전체" };

const makeStyles = (t: Theme) => StyleSheet.create({
  container: {
    flex: 1,
  },
  barRow: {
    flexDirection: "row",
    alignItems: "center",
    gap: space.sm,
  },
  resumeBar: {
    paddingHorizontal: space.lg,
    paddingVertical: space.sm,
    backgroundColor: t.surface,
    flexDirection: "row",
    alignItems: "center",
    gap: space.sm,
  },
  resumeTextCol: {
    flex: 1,
    gap: 2,
  },
  resumeLabel: {
    fontSize: font.size.xs,
    color: t.subtext,
  },
  resumeCmd: {
    fontSize: font.size.xs,
    fontFamily: "monospace",
    color: t.text,
  },
  todoBar: {
    minHeight: TOUCH,
    justifyContent: "center",
    paddingHorizontal: space.lg,
    paddingVertical: space.sm,
    backgroundColor: t.surface,
    borderBottomWidth: StyleSheet.hairlineWidth,
    borderBottomColor: t.border,
  },
  todoTitle: {
    flex: 1,
    fontSize: font.size.sm,
    fontWeight: font.weight.bold,
    color: t.text,
  },
  todoItem: {
    fontSize: font.size.xs,
    color: t.subtext,
    marginTop: space.xs,
    marginLeft: 26,
  },
  limitBar: {
    flexDirection: "row",
    alignItems: "center",
    flexWrap: "wrap",
    gap: space.sm,
    paddingHorizontal: space.lg,
    paddingVertical: space.sm,
    borderBottomWidth: StyleSheet.hairlineWidth,
    borderBottomColor: t.border,
  },
  limitText: {
    fontSize: font.size.xs,
    fontWeight: font.weight.bold,
  },
  listWrap: {
    flex: 1,
  },
  list: {
    flex: 1,
  },
  listContent: {
    padding: space.lg,
    gap: space.sm,
  },
  userBubble: {
    alignSelf: "flex-end",
    backgroundColor: t.bubbleUser,
    borderRadius: radius.lg,
    borderBottomRightRadius: space.xs,
    paddingHorizontal: space.md,
    paddingVertical: 10,
    maxWidth: "85%",
  },
  assistantBubble: {
    alignSelf: "flex-start",
    backgroundColor: t.bubbleBot,
    borderRadius: radius.lg,
    borderBottomLeftRadius: space.xs,
    paddingHorizontal: space.md,
    paddingVertical: 10,
    maxWidth: "85%",
  },
  userText: {
    fontSize: 15,
    lineHeight: 22,
    color: t.text,
  },
  toolOnlyRow: {
    alignSelf: "flex-start",
    maxWidth: "85%",
  },
  toolButton: {
    flexDirection: "row",
    alignItems: "center",
    gap: space.xs,
    alignSelf: "flex-start",
    height: 28,
    paddingHorizontal: space.sm,
    borderRadius: radius.pill,
    borderWidth: StyleSheet.hairlineWidth,
    borderColor: t.border,
    backgroundColor: t.surface,
    marginBottom: space.xs,
  },
  toolButtonText: {
    fontSize: font.size.xs,
    color: t.subtext,
  },
  toolDetail: {
    fontSize: 11,
    color: t.subtext,
    fontFamily: "monospace",
    backgroundColor: t.code,
    borderRadius: radius.sm,
    paddingHorizontal: space.sm,
    paddingVertical: space.xs,
    marginBottom: space.xs,
  },
  verdictRow: {
    alignItems: "center",
    paddingVertical: space.sm,
    gap: 2,
  },
  verdictText: {
    fontSize: font.size.sm,
    fontWeight: font.weight.bold,
    color: t.text,
  },
  changedFiles: {
    fontSize: font.size.xs,
    color: t.subtext,
  },
  errorRow: {
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "center",
    gap: space.xs,
    paddingVertical: space.xs,
  },
  sendError: {
    backgroundColor: t.bg,
    paddingHorizontal: space.lg,
  },
  errorText: {
    flexShrink: 1,
    fontSize: font.size.sm,
    color: t.danger,
  },
  diffBox: {
    backgroundColor: t.surface,
    borderRadius: radius.md,
    borderWidth: StyleSheet.hairlineWidth,
    borderColor: t.border,
    paddingHorizontal: space.md,
    paddingVertical: space.xs,
    marginTop: space.xs,
  },
  diffHeader: {
    flexDirection: "row",
    alignItems: "center",
    gap: space.sm,
    minHeight: TOUCH,
  },
  diffTitle: {
    flex: 1,
    fontSize: font.size.sm,
    fontWeight: font.weight.bold,
    color: t.text,
  },
  chipRow: {
    flexDirection: "row",
    flexWrap: "wrap",
    gap: space.sm,
    paddingVertical: space.sm,
  },
  diffEntryRow: {
    flexDirection: "row",
    alignItems: "center",
    gap: space.sm,
    minHeight: 40,
  },
  diffStatus: {
    width: 20,
    fontSize: font.size.xs,
    fontFamily: "monospace",
    fontWeight: font.weight.bold,
    color: t.subtext,
  },
  diffEntry: {
    flex: 1,
    fontSize: font.size.xs,
    color: t.text,
  },
  diffText: {
    fontSize: 10,
    fontFamily: "monospace",
    color: t.text,
    backgroundColor: t.code,
    borderRadius: radius.sm,
    padding: space.sm,
    marginVertical: space.xs,
  },
  jumpDownButton: {
    position: "absolute",
    right: space.lg,
    bottom: 14,
    width: TOUCH,
    height: TOUCH,
    borderRadius: TOUCH / 2,
    backgroundColor: t.surfaceAlt,
    borderWidth: StyleSheet.hairlineWidth,
    borderColor: t.border,
    alignItems: "center",
    justifyContent: "center",
  },
  permCard: {
    borderTopWidth: StyleSheet.hairlineWidth,
    borderTopColor: t.border,
    backgroundColor: t.surface,
    paddingHorizontal: space.lg,
    paddingVertical: space.md,
    gap: space.sm,
  },
  permTitle: {
    flex: 1,
    fontSize: font.size.md,
    fontWeight: font.weight.bold,
    color: t.text,
  },
  permBody: {
    fontSize: font.size.xs,
    fontFamily: "monospace",
    color: t.subtext,
  },
  permButtons: {
    flexDirection: "row",
    gap: space.sm,
  },
  permButton: {
    flex: 1,
    paddingHorizontal: space.sm,
  },
  permReason: {
    minHeight: 44,
    paddingHorizontal: space.md,
    borderRadius: radius.md,
    backgroundColor: t.bg,
    fontSize: font.size.sm,
    color: t.text,
  },
  // 컨트롤 위 칩 줄(답·계획 승인·커밋·프리셋) — 키보드 위에 겹쳐 뜨므로 바탕을 칠한다
  chipBar: {
    flexDirection: "row",
    flexWrap: "wrap",
    gap: space.sm,
    paddingHorizontal: space.lg,
    paddingVertical: space.sm,
    borderTopWidth: StyleSheet.hairlineWidth,
    borderTopColor: t.border,
    backgroundColor: t.bg,
  },
  chipText: {
    fontSize: font.size.sm,
    color: t.text,
  },
  attachRow: {
    flexDirection: "row",
    alignItems: "center",
    gap: space.md,
    paddingHorizontal: space.lg,
    paddingVertical: space.md,
    borderTopWidth: StyleSheet.hairlineWidth,
    borderTopColor: t.border,
    backgroundColor: t.bg,
  },
  thumbWrap: {
    position: "relative",
  },
  thumb: {
    width: 56,
    height: 56,
    borderRadius: radius.sm,
    backgroundColor: t.surfaceAlt,
  },
  thumbRemove: {
    position: "absolute",
    top: -6,
    right: -6,
    width: 20,
    height: 20,
    borderRadius: 10,
    // 사진 위에 얹히므로 테마와 무관하게 어두운 바탕·흰 아이콘
    backgroundColor: "rgba(0,0,0,0.65)",
    alignItems: "center",
    justifyContent: "center",
  },
  inputBar: {
    paddingHorizontal: space.md,
    paddingTop: space.sm,
    gap: space.sm,
    borderTopWidth: StyleSheet.hairlineWidth,
    borderTopColor: t.border,
    // 키보드가 열리면 입력바가 리스트 위에 겹쳐 뜨므로, 투명하면 뒤의 변경 목록이 비쳐 컨트롤과 뭉개진다
    backgroundColor: t.bg,
  },
  controlRow: {
    flexDirection: "row",
    alignItems: "center",
    flexWrap: "wrap",
    gap: space.sm,
  },
  planRow: {
    flexDirection: "row",
    alignItems: "center",
    gap: space.xs,
    height: TOUCH,
    paddingLeft: space.xs,
  },
  planLabel: {
    fontSize: font.size.sm,
    color: t.text,
  },
  iconRow: {
    flexDirection: "row",
    alignItems: "center",
  },
  inputRow: {
    flexDirection: "row",
    alignItems: "flex-end",
    gap: space.sm,
  },
  input: {
    flex: 1,
    minHeight: TOUCH,
    maxHeight: 120,
    paddingHorizontal: space.lg,
    paddingTop: 12,
    paddingBottom: 12,
    borderRadius: 24,
    backgroundColor: t.surface,
    borderWidth: StyleSheet.hairlineWidth,
    borderColor: t.border,
    fontSize: font.size.md,
    color: t.text,
  },
  sendButton: {
    width: TOUCH,
    height: TOUCH,
    borderRadius: TOUCH / 2,
    backgroundColor: t.accent,
    alignItems: "center",
    justifyContent: "center",
  },
  queueButton: {
    height: TOUCH,
  },
});
