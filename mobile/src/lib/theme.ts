import { useColorScheme } from "react-native";

/** 화면 공통 팔레트 — useTheme()로 시스템 라이트/다크를 따른다. */
export interface Theme {
  text: string;
  subtext: string;
  border: string;
  /** 살짝 떠 있는 박스(요약/리줌바 등) */
  box: string;
  /** 내 말풍선 / 상대 말풍선 */
  bubbleUser: string;
  bubbleBot: string;
  /** 칩·툴 버튼류 배경 */
  chip: string;
  chipText: string;
  /** 모노스페이스 코드/디프 배경 */
  mono: string;
  /** 입력창 글자/플레이스홀더 */
  inputText: string;
  placeholder: string;
  /** 강조(파랑) — 라이트/다크 공통 사용 가능 */
  accent: string;
}

export const lightTheme: Theme = {
  text: "#111",
  subtext: "#666",
  border: "#ccc",
  box: "#f7f7f7",
  bubbleUser: "#dcefff",
  bubbleBot: "#f0f0f0",
  chip: "#ececec",
  chipText: "#555",
  mono: "#f2f2f2",
  inputText: "#111",
  placeholder: "#888",
  accent: "#2f6fed",
};

export const darkTheme: Theme = {
  text: "#e8e8e8",
  subtext: "#9a9a9a",
  border: "#3a3a3a",
  box: "#1c1c1e",
  bubbleUser: "#1e3a5f",
  bubbleBot: "#2a2a2c",
  chip: "#2c2c2e",
  chipText: "#bbb",
  mono: "#232325",
  inputText: "#eee",
  placeholder: "#777",
  accent: "#5b8def",
};

export function useTheme(): Theme {
  return useColorScheme() === "dark" ? darkTheme : lightTheme;
}
