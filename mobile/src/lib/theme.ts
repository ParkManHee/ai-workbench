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
  border: string;
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
  bubbleUser: string;
  bubbleBot: string;
  /** 모노스페이스 코드/디프 바탕 */
  code: string;
  /** 시트 뒤 어둡게 */
  scrim: string;
}

export const lightTheme: Theme = {
  bg: "#ffffff",
  surface: "#f4f5f7",
  surfaceAlt: "#e9ebef",
  elevated: "#ffffff",
  border: "#dfe2e7",
  text: "#14171c",
  subtext: "#5c636e",
  placeholder: "#737a84",
  accent: "#2563eb",
  onAccent: "#ffffff",
  accentSoft: "#ebf1ff",
  success: "#1e8e3e",
  warning: "#b25e00",
  danger: "#d0312d",
  onDanger: "#ffffff",
  bubbleUser: "#dfe9ff",
  bubbleBot: "#f4f5f7",
  code: "#eceef2",
  scrim: "rgba(0,0,0,0.45)",
};

export const darkTheme: Theme = {
  bg: "#111317",
  surface: "#1b1e24",
  surfaceAlt: "#262a32",
  elevated: "#1e2128",
  border: "#30353e",
  text: "#e8eaed",
  subtext: "#9aa1ab",
  placeholder: "#7a818b",
  // 다크에선 밝은 파랑 + 어두운 글자 — 흰 글자를 올리면 대비가 3:1 아래로 떨어진다
  accent: "#8ab4f8",
  onAccent: "#0b1a33",
  accentSoft: "#1f3358",
  success: "#81c995",
  warning: "#fbbc5d",
  danger: "#f28b82",
  onDanger: "#2b0b08",
  bubbleUser: "#1f3a66",
  bubbleBot: "#1b1e24",
  code: "#22262d",
  scrim: "rgba(0,0,0,0.6)",
};

export function useTheme(): Theme {
  return useColorScheme() === "dark" ? darkTheme : lightTheme;
}

/** 4pt 격자 — 화면 가장자리는 lg */
export const space = { xs: 4, sm: 8, md: 12, lg: 16, xl: 24 } as const;

export const radius = { sm: 8, md: 12, lg: 16, pill: 999 } as const;

export const font = {
  size: { xs: 12, sm: 14, md: 16, lg: 18, xl: 22 },
  weight: { regular: "400", medium: "500", bold: "600" },
} as const;

/** 누르는 곳 최소 크기(Material 48dp) */
export const TOUCH = 48;
