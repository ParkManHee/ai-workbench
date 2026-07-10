# ai-workbench 기능 백로그

2026-07-10, 4개 관점(실사용 UX·안정성/운영·경쟁 비교·스펙 이월) 에이전트 조사 통합.
완료 시 체크하고 필요하면 재우선순위화한다.

## Tier 1 — "폰 감독 루프" 완성 (2026-07-10 완료)

- [x] **질문 대기(🔴) 전용 푸시** — 완료가 아니라 "답 필요해서 멈춤"을 알림으로 구분 (S~M)
- [x] **hung run 자동 타임아웃** — 멈춘 claude가 락을 무기한 잡는 문제. 초과 시 cancel + .done (M)
- [x] **실행 중 후속 지시(턴 큐잉)** — running 중 409 대신 큐 적재, 턴 종료 시 자동 resume 전달 (M)
- [x] **어디서든 취소** — 앱 재시작·다른 기기에서도 활성 run에 attach해 취소 (M)
- [x] **툴 권한 원격 승인/거부** — Bash/Edit 허가를 폰에서 버튼으로. 공식 RC·Happy·Omnara 보유, 무인 실행이 멈추는 최대 원인 (L)

## Tier 2 — 무인 운영 안정화

- [x] 데몬 launchd 상주화 (RunAtLoad + KeepAlive + 파일 로그) (M)
- [x] `.awb-runs`/`.awb-uploads` GC — N일/M개 초과 정리 (S)
- [x] 페어링 기기 관리 — GET /devices + revoke 라우트 + 폰 설정 UI (M)
- [x] 데몬 진단(/info 확장: version·uptime·활성 run) — 버전·uptime·활성 run·최근 에러 + tracing 파일 로깅 (M)
- [x] 앱 버전 협상 — /info에 min_app_version, 폰 업데이트 배너 (M)

## Tier 3 — 저공수·고체감 UX

- [x] localhost dev 서버 폰 프리뷰 — 열린 포트 감지 → Tailscale URL 웹뷰 (S)
- [ ] plan 승인 원탭 — 계획 출력 후 "이대로 진행" 칩 (S)
- [ ] 실행 중 diff 미리보기 (S)
- [ ] 음성 입력(STT) (S)
- [ ] 원격 슬래시커맨드 /model·/compact·/usage (S)
- [ ] Git 액션 버튼 — 커밋/푸시/PR 원탭 + PR URL 표시 (S~M)
- [ ] Todo/계획 진행 카드 — TodoWrite 이벤트 파싱해 "지금 뭐 하는 중/남은 단계" (M)

## Tier 4 — 대형·장기

- [ ] 병렬 세션 + git worktree 자동 격리 (공식 RC --spawn worktree의 로컬 번안) (L)
- [ ] diff 인라인 코멘트 → 에이전트 피드백 (M)
- [ ] 데스크톱 Plan 3 잔여 — 데스크톱 diff 뷰·스냅샷 롤백·redaction·권한 하드닝·reader-settings.json (L)
- [ ] iOS(APNs) (L)
- [ ] Wake-on-LAN (M)
- [ ] 세션 검색/이름·대화 영구저장 (M~L)

## Tier 5 — 폴리시 소품 (일괄 처리 후보)

- [x] DeviceStore 원자적 쓰기 (temp+rename)
- [ ] chat 에러 세분화 (409 사유 코드)
- [ ] runlog UTF-8 청크 경계 하드닝
- [ ] list_sessions 페이지네이션
- [ ] thinking 블록 접기
- [ ] PC label 편집
- [ ] 푸시 영수증 확인 + 죽은 토큰 프루닝
- [ ] 정교한 WS offset 재접속 (offset=0 재생 개선)
- [ ] caffeinate 비-macOS 분기
- [ ] Mac 앞 presence 감지 시 푸시 억제 (공식 RC 아이디어)
