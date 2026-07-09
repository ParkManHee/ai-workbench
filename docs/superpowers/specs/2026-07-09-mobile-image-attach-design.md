# 모바일 이미지 첨부 (폰→Mac→claude) 설계

날짜: 2026-07-09 · 상태: 승인됨(대화에서 A안 승인)

## 목적

폰 채팅에서 이미지(주로 스크린샷)를 첨부해 Mac의 claude 실행에 전달한다.
PC에서 이미지를 붙여넣는 것과 같은 효과를 낸다.

## 흐름

1. 폰: 입력바의 🖼 버튼 → 갤러리 다중 선택(최대 3장, quality 0.8 압축)
   → 입력바 위 썸네일 미리보기(✕ 제거 가능)
2. 전송: 각 이미지를 `POST /upload`(인증 필수)로 업로드
   → 데몬이 `~/.claude/.awb-uploads/<epoch>-<seq>.<ext>`에 저장, `{path}` 반환
3. `/chat` 프롬프트 끝에 `[첨부 이미지: <경로> — Read 도구로 확인]` 줄 추가
   → 실행된 에이전트가 Read 도구로 이미지를 봄
4. 내 말풍선에는 입력 텍스트 + `🖼 이미지 N장` 표시

## 서버 (`crates/awb-server`)

- `POST /upload?ext=jpg|jpeg|png|webp` — body는 raw bytes(octet-stream), 한도 15MB
  (axum 기본 2MB 한도를 이 라우트만 상향). 확장자 화이트리스트 외는 400.
- 저장 디렉터리는 홈 아래 고정(`~/.claude/.awb-uploads/`) — 경로 주입 불가.
- 응답 `{ "path": "/Users/.../.awb-uploads/....jpg" }`.

## 모바일 (`mobile/`)

- `expo-image-picker` 추가(네이티브 모듈 → dev client APK 리빌드 필요, 1회).
  Android 13+는 시스템 포토 픽커라 권한 불필요.
- `api.ts`: `upload(uri, ext)` — 로컬 uri를 blob으로 읽어 전송.
- `chat/[project].tsx`: images 상태, 썸네일 행, handleSend에서 업로드 후 경로 첨부.
  업로드 실패 시 전송 중단 + 에러 표시(부분 업로드로 실행하지 않음).

## 대안 (기각)

- 카메라 촬영만(expo-camera 기존 포함): 리빌드 불필요하나 갤러리 불가 — 주 용례 미충족.
- base64를 /chat JSON에 포함: 큰 사진에 비효율, 픽커는 어차피 필요.

## 테스트

- 서버: 업로드 무인증 401 / 정상 업로드 저장·경로 반환 / 허용 외 확장자 400.
- 폰 수동: 선택→미리보기→전송→에이전트가 이미지를 읽고 답하는지, 3장/제거/취소 동작.
