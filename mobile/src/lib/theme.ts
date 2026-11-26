import { useColorScheme } from "react-native";

/** 화면 공통 색 — useTheme()로 시스템 라이트/다크를 따른다. 화면에서 hex 를 직접 쓰지 않는다. */
export interface Theme {
  /** 화면 바탕 */
  bg: string;
  /** 카드·시트·상대 말풍선처럼 바탕 위에 한 겹 뜬 면 */
  surface: string;
  /** 칩·입력칸처럼 surface 보다 한 단계 짙은 면 */
  surfaceAlt: string;
  /** 시트·다이얼로그 바탕 — 다크에선 바탕보다 밝아야 scrim 위에서 떠 보인다 */
  elevated: string;
  /** 구분선·카드 테두리 — 면을 나누는 장식이라 옅다 */
  border: string;
  /** 입력칸 테두리 — 칸의 경계가 곧 누를 곳이라 bg·surface·elevated 위에서 3:1 이상(WCAG 1.4.11) */
  inputBorder: string;
  text: string;
  subtext: string;
  placeholder: string;
  accent: string;
  /** accent 바탕 위 글자 */
  onAccent: string;
  /** 선택된 칩 바탕 */
  accentSoft: string;
  success: string;
  warning: string;
  danger: string;
  /** danger 바탕 위 글자 */
  onDanger: string;
  /** 내 말풍선 — 강조색으로 채워 대화에서 내 줄이 먼저 보인다 */
  bubbleUser: string;
  /** bubbleUser 위 글자 */
  onBubbleUser: string;
  bubbleBot: string;
  /** 상태 배지 바탕 — 글자는 success·warning 그대로(4.5:1 이상) */
  successSoft: string;
  warningSoft: string;
  /** 모노스페이스 코드/디프 바탕 */
  code: string;
  /** 시트 뒤 어둡게 */
  scrim: string;
  /** 면 구분 그림자(boxShadow) — card: 카드·말풍선·칩 묶음, raised: 권한 카드·시트처럼 한 겹 더 뜬 것. 다크는 그림자가 묻혀 진하게 */
  shadowCard: string;
  shadowRaised: string;
}

// 시안 8 「채팅 앱」 — 대비표: target/mockups/gen.py(아바타·꼬리 말풍선·부드러운 그림자)
export const lightTheme: Theme = {
  bg: "#f2f6f5",
  surface: "#ffffff",
  surfaceAlt: "#e4edea",
  elevated: "#ffffff",
  border: "#dbe4e1",
  inputBorder: "#7f8c88",
  text: "#15201d",
  subtext: "#55625e",
  placeholder: "#5f6c68",
  accent: "#0b7a65",
  onAccent: "#ffffff",
  accentSoft: "#d7efe9",
  success: "#13733a",
  warning: "#a05000",
  danger: "#c62f2f",
  onDanger: "#ffffff",
  bubbleUser: "#0b7a65",
  onBubbleUser: "#ffffff",
  bubbleBot: "#ffffff",
  successSoft: "#dcf3e3",
  warningSoft: "#fde8d4",
  code: "#eef3f1",
  scrim: "rgba(0,0,0,0.4)",
  shadowCard: "0px 1px 2px rgba(0,0,0,0.06), 0px 4px 14px rgba(0,0,0,0.06)",
  shadowRaised: "0px 2px 10px rgba(0,0,0,0.10)",
};

export const darkTheme: Theme = {
  bg: "#0f1716",
  surface: "#18221f",
  surfaceAlt: "#22302c",
  elevated: "#1d2925",
  border: "#2a3834",
  inputBorder: "#6f807b",
  text: "#e6efec",
  subtext: "#9fb0ab",
  placeholder: "#8a9b96",
  // 다크에선 밝은 청록 + 어두운 글자 — 흰 글자를 올리면 대비가 3:1 아래로 떨어진다
  accent: "#4fd1b2",
  onAccent: "#032019",
  accentSoft: "#143b33",
  success: "#6fdc8c",
  warning: "#ffb468",
  danger: "#ff8a80",
  onDanger: "#2b0806",
  bubbleUser: "#136b5a",
  onBubbleUser: "#ffffff",
  bubbleBot: "#18221f",
  successSoft: "#163d25",
  warningSoft: "#43301a",
  code: "#13201d",
  scrim: "rgba(0,0,0,0.6)",
  shadowCard: "0px 1px 2px rgba(0,0,0,0.4), 0px 6px 16px rgba(0,0,0,0.35)",
  shadowRaised: "0px 2px 12px rgba(0,0,0,0.5)",
};

/** 프로젝트 이니셜 아바타 — 이름으로 색을 고른다. 흰 글자 대비 5.2:1 이상(라이트·다크 같은 색) */
export const avatar = {
  fg: "#ffffff",
  bg: ["#3b5bdb", "#b4441a", "#6b46c1", "#0b7a65", "#c2255c", "#2b7a3e", "#1864ab", "#866400"],
} as const;

/** 카메라 영상·사진 위에 얹는 요소 — 영상 밝기가 제각각이라 테마와 무관하게 어두운 바탕·흰 글자 */
export const media = { scrim: "rgba(0,0,0,0.7)", fg: "#ffffff", frame: "rgba(255,255,255,0.9)" } as const;

export function useTheme(): Theme {
  return useColorScheme() === "dark" ? darkTheme : lightTheme;
}

/** 4pt 격자 — 화면 가장자리는 lg. xxs 는 제목·부제처럼 붙어 읽히는 글줄 사이 */
export const space = { xxs: 2, xs: 4, sm: 8, md: 12, lg: 16, xl: 24 } as const;

/** xs 는 글줄 안 코드 조각 · tail 은 말풍선 꼬리 쪽 모서리 — 원·알약 모양은 pill */
export const radius = { xs: 8, sm: 14, md: 20, lg: 24, tail: 6, pill: 999 } as const;

export const font = {
  /** body: 대화 말풍선 본문 · code: 도구 상세·diff·코드 블록 모노스페이스 — 많은 줄을 한 화면에 */
  size: { code: 12, xs: 12, sm: 14, body: 16, md: 16, lg: 18, xl: 24 },
  weight: { regular: "400", medium: "500", bold: "600" },
  /** body: 16px 긴 글(말풍선·원문 선택) — 시안 줄 간격 1.45 */
  lineHeight: { body: 23 },
} as const;

/** 누르는 곳 최소 크기(Material 48dp) */
export const TOUCH = 48;
