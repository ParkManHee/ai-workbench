# 모바일 앱 디자인 전체 개편

## 의도
사용자 요청: "원하는 디자인은 딱히 없고, 깔끔하고 사용성 좋기만 하면 된다 — 전체적으로 수정".
지금 앱은 색만 `src/lib/theme.ts` 로 모여 있고 간격·모서리·글자 크기·버튼 모양이 화면마다 제각각이다.

## 목표 / 비목표
- 목표: 화면 7개(`_layout`·`index`·`projects`·`sessions/[project]`·`chat/[project]`·`pair`·`devices`)와
  시트 3개(`ChoiceSheet`·`GitSheet`·`TextSelectModal`)가 한 벌의 토큰·공통 부품을 쓴다
- 목표: 누르는 곳 48dp 이상(hitSlop 포함), 상태(작업 중·답 필요·쉼)는 어느 목록이든 같은 배지
- 비목표: 기능·화면 흐름·API·데몬 변경, 새 화면
- 반목표(이러면 실패): 기능이 하나라도 사라지거나 동작이 바뀜 · 다크/라이트 중 한쪽이 깨짐 · 키보드·입력바 겹침 재발

## 방법(검토한 것)
- **A. 토큰 + 공통 부품, StyleSheet 유지 — 채택.** 색은 이미 대부분 theme 를 거친다(하드코딩 15곳). 1~1.5일
- B. 컴포넌트 킷(NativeWind·Tamagui) — 전 화면 재작성·의존성 추가, 요청 대비 과함
- C. 색만 교체 — 간격·터치 영역·모양 불일치가 남는다
- 참고 지침: Expo 공식 스킬 `expo-design-system`(토큰·드리프트 점검)·`expo-native-ui`(Android 는 Material 3). 설치하지 않고 지침만 따른다

## 설계
- `src/lib/theme.ts`: 색 토큰(bg·surface·surfaceAlt·border·text·subtext·accent·onAccent·success·warning·danger·말풍선·코드)
  다크/라이트 두 벌 + `space`(4·8·12·16·24) · `radius`(8·12·16·pill) · `font`(12·14·16·18·22, 굵기) 상수.
  기존 `Theme` 키를 쓰는 곳은 새 키로 옮기고 안 쓰게 된 키는 지운다
- 공통 부품 `src/components/ui.tsx`(필요한 것만): Button(primary·secondary·danger, 높이 48) · Chip · ListRow(제목·부제·오른쪽, 높이 56+) ·
  StatusBadge(working·waiting·idle) · SectionHeader · EmptyState · SheetFrame(시트 3개 공통 틀)
- 헤더: `_layout` 의 Stack screenOptions 로 배경·글자색을 토큰에 맞춘다
- 채팅: 말풍선 모서리 16·최대 폭 85%, 내 말풍선은 accent 계열, 도구·변경 목록은 옅은 카드, 입력칸은 둥근 필 + 원형 전송 버튼,
  컨트롤 칩은 Chip 하나로 통일. 기존 접기 동작(대화창 누르기·끌기 → 입력칸만)은 그대로
- 아이콘: 이모지(🖼🌐🎤📦 등)는 기기마다 모양이 달라 정돈돼 보이지 않는다 → `npx expo install @expo/vector-icons` 로
  추가(현재 node_modules 에 없음 — 새 의존성 1개, Expo 관리 패키지). 쓰는 곳만
- 판단은 구현자 몫: 정확한 색 값, 부품 세부 props, 화면별 배치 미세 조정

## 확인
- 에뮬레이터(Medium_Phone_API_36.1)에서 화면 7개 + 시트 3개를 다크·라이트로 스크린샷, 겹침·잘림 없음
- `tsc --noEmit` 0 · vitest 통과
