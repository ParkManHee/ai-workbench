# ai-workbench Mobile — 폰↔Mac 원격 실행 설계 (v1)

> 작성 2026-07-07 · 브레인스토밍(superpowers:brainstorming) 결과. 다음 단계: writing-plans.
> 선행: ai-workbench 데스크톱 v0(Plan1+2) 완료, 브랜치 `plan2-execution`(미머지, 유지).

## 목표

핸드폰 앱으로 **어디서나(인터넷) Mac에 연결해 프롬프트를 입력하고 답변을 실시간으로 확인**한다. 카페·외부에서도 폰으로 Mac의 `claude`를 멀티턴 대화로 구동하고, 실행 취소·plan 모드·완료 판정·git 변경 요약을 폰에서 다룬다. 작업 완료는 앱이 닫혀 있어도 푸시로 통지한다.

## 확정된 핵심 결정

| 항목 | 결정 |
|---|---|
| 연결 범위 | 어디서나(인터넷) |
| 전송 | Tailscale 메시 VPN (릴레이 서버 없음, 종단간 암호화, tailnet 바인드) |
| 모바일 형태 | 네이티브 앱 (PWA/Tauri 아님) |
| 네이티브 스택 | React Native + Expo |
| 대상 OS | **Android 우선**(EAS build → APK 직접 설치), iOS는 v2 |
| 상호작용 | 멀티턴 대화(채팅형), 데몬이 `claude --resume`으로 세션 관리 |
| 인증 | Tailscale(네트워크) + QR 페어링 토큰(앱 계층) 이중 방어 |
| 스트리밍 | WebSocket 실시간 토큰 스트림(`claude --output-format stream-json`), 재접속 이어보기 |
| 완료 푸시 | **v1 포함**, Expo 푸시 릴레이(경로 A) — FCM V1 서비스계정 JSON은 EAS에 1회 업로드, 데몬은 ExpoPushToken만 POST |
| 데몬 형태 | 코어를 lib로 추출 + 독립 데몬 바이너리와 데스크톱 앱 **양쪽 진입점**, 서버 단일 인스턴스 가드 |
| 잠자기 대응 | **깨어있기 유지만 v1**(실행 중 자동 sleep 방지 + 폰 토글), WoL은 v2 |
| 레포 | **모노레포** — ai-workbench 안에 desktop/daemon/mobile 전부 |
| 폰 v1 기능 | 기본(프로젝트 선택·프롬프트·스트리밍·멀티턴) + 실행 취소 + plan 토글 + 완료 배지/판정 + git 변경 요약 |

## 아키텍처

```
┌─────────────────────┐        Tailscale 메시 VPN         ┌──────────────────────────┐
│   폰 (RN + Expo)     │ ←── HTTP/WS (tailnet 주소) ───→  │   Mac                       │
│  - 페어링(QR 1회)     │                                   │  ┌──────────────────────┐  │
│  - 프로젝트 목록      │                                   │  │ awb-server (신규)     │  │
│  - 채팅(멀티턴)       │        완료 시 Expo 푸시          │  │  HTTP/WS, tailnet bind │  │
│  - 스트리밍 수신      │ ←──(exp.host, 인터넷)──────────  │  │  QR 페어링·토큰 인증   │  │
│  - 취소/plan/판정     │                                   │  │  claude --resume 세션  │  │
│  - git 요약           │                                   │  │  stream-json → WS 브릿지│  │
│  - 깨어있기 토글      │                                   │  │  완료 시 Expo 푸시 발송 │  │
└─────────────────────┘                                   │  │  전원 어서션(caffeinate)│  │
                                                            │  └─────────┬────────────┘  │
                                                            │            │ 재사용         │
                                                            │  ┌─────────▼────────────┐  │
                                                            │  │ awb-core (신규 lib)   │  │
                                                            │  │ scan/runner/runlog/   │  │
                                                            │  │ lock/preflight/paths  │  │
                                                            │  └─────────┬────────────┘  │
                                                            │   ┌────────▼───────────┐  │
                                                            │   │ ai-workbench 앱     │  │
                                                            │   │ (Tauri, 기존)       │  │
                                                            │   └─────────────────────┘  │
                                                            └──────────────────────────┘
```

**모노레포 구조 (ai-workbench 안):**
```
crates/
  awb-core/       기존 src-tauri/src의 scan·runner·runlog·lock·preflight·paths를 lib로 추출
  awb-server/     신규 데몬 바이너리(HTTP/WS·페어링·세션·스트림·푸시·전원). launchd/CLI 진입점.
src-tauri/        기존 데스크톱 앱 — awb-core 의존으로 리팩터(로직 중복 제거)
mobile/           신규 RN+Expo 앱 (채팅 UI·WS 클라이언트·푸시 등록)
```

### 컴포넌트별 책임 (한 줄)

- **awb-core** — 프로젝트 스캔·실행·로그tail·완료판정·락·프리플라이트. 순수 로직, UI/전송 무관.
- **awb-server** — 전송(HTTP/WS)·인증(QR/토큰)·멀티턴 세션(`claude --resume`)·스트림 브릿지·완료 푸시·전원 어서션.
- **mobile** — 화면·입력·스트림 표시. 서버 API 소비만.

### 서버 단일 인스턴스 가드

서버 로직은 `awb-server` 또는 데스크톱 앱이 실행하되, **tailnet 포트를 먼저 잡은 프로세스만 서빙**. 보통은 launchd 데몬이 상주 서버, 데스크톱 앱은 데몬이 없을 때만 자체 서버 기동. 포트가 이미 바인드돼 있으면 신규 서버는 기동하지 않고 "기존 서버 사용"으로 로깅. claude 실행 이중방지는 기존 **runlock**(FNV realpath 키)이 앱·폴러·데몬 삼자에 걸쳐 그대로 담당.

## awb-server: API

모두 tailnet 바인드 + Bearer 토큰 인증 (페어링 엔드포인트만 예외).

| 메서드 | 경로 | 용도 |
|---|---|---|
| `GET` | `/pair?code=<qr>` | QR 페어링 → 디바이스 토큰 발급 (인증 예외, 1회성·만료 코드) |
| `GET` | `/projects` | 프로젝트 목록 (awb-core scan + preflight 배지) |
| `POST` | `/chat/:project` | 새 턴 `{prompt, plan?}` → `{session_id, run_id}`. 락 획득 후 detached `claude` 기동 |
| `WS` | `/stream/:run_id?offset=N` | 실시간 토큰 스트림 + 완료 이벤트. offset 재접속 이어보기 |
| `POST` | `/cancel/:run_id` | 실행 취소 (awb-core cancel_run: PGID TERM→KILL) |
| `GET` | `/status/:run_id` | 완료 판정(verdict·exit·changed_files) — 재접속/폴백 |
| `GET` | `/diff/:project` | git 변경 요약(파일·+/- 라인). **데몬 신규 로직** |
| `POST` | `/push/register` | Expo push 토큰 등록·저장 |
| `POST` | `/awake` | `{on: bool}` 깨어있기 유지 토글 (전원 어서션) |

### 멀티턴 세션 (`claude --resume`)

- 프로젝트별 **세션 1개** 유지. 첫 턴 `claude -p ... --output-format stream-json`, 이후 턴 `--resume <session_id>`.
- `session_id`는 첫 턴 stream-json init 이벤트에서 추출해 `~/.claude/.awb-sessions/<project>.json`에 저장.
- 세션 상태 `idle | running`. running 중 새 턴 요청은 락 보유로 409 거부(폰에 "실행 중, 취소 후 재시도"). v1은 큐잉 안 함.

### stream-json → WS 브릿지

- 래퍼가 `claude --output-format stream-json` 실행 → 데몬이 stdout을 **줄 단위(JSONL) 파싱**해 WS로 push.
- WS 이벤트 타입: `token`(부분 텍스트) · `tool_use`(툴 호출 요약) · `done`(exit+verdict) · `error`.
- **재접속 이어보기**: 각 run 이벤트를 로그파일에 append(기존 runlog 오프셋 방식 확장). WS 연결 시 `?offset=N`부터 재생 → 놓친 토큰 catch-up.
- **부분 줄 처리**: 줄 완성 전까지 버퍼링, 깨진 줄은 skip + `error` 이벤트 로깅.

## 인증 & QR 페어링 (이중 방어)

**방어선 1 — Tailscale (네트워크 계층):** 데몬은 tailnet 인터페이스(`100.x.x.x`)에만 바인드, `0.0.0.0` 금지. tailnet 밖에서는 포트 미노출.

**방어선 2 — 페어링 토큰 (앱 계층):** Tailscale를 쓰는 다른 기기의 실수 접근까지 차단.

**페어링 흐름 (1회성):**
```
1. 데몬 최초 기동 → 페어링 코드 생성(짧은 랜덤 + 만료 60s)
       → QR 표시 (데스크톱 앱 실행 중이면 앱 창, 데몬 단독이면 터미널 ASCII QR + ~/.claude/.awb-pair.png)
2. 폰 앱 첫 실행 → QR 스캔 → GET /pair?code=<qr> (tailnet 경유)
3. 데몬: 코드 검증(유효·미만료) → 디바이스 토큰(랜덤 32B) 발급
       → ~/.claude/.awb-devices.json  {device_id, token_hash(SHA-256), paired_at, label}
4. 폰: 토큰을 Expo SecureStore(Android Keystore)에 저장
5. 이후 모든 요청: Authorization: Bearer <token> (없으면 401)
```

**세부:** 토큰은 **해시(SHA-256)만** 저장, 원문 저장 안 함. 다중 기기 목록 관리(폰1·폰2). revoke는 파일 항목 삭제(CLI `awb-server unpair <id>`). 디바이스 토큰은 장기(무만료, revoke로만 해제), 페어링 코드만 단기. `/push/register`도 이 Bearer 토큰으로 인증 → 페어링된 기기만 푸시 등록 가능.

## 모바일 앱 (RN + Expo)

**화면 (v1, 3화면):**
```
① 페어링          ② 프로젝트 목록        ③ 채팅(프로젝트별)
 QR 스캔 →         scan+preflight 배지    상단: 프로젝트명·상태(idle/running)·깨어있기 토글
 토큰 저장         프로젝트 탭 →          본문: 멀티턴 말풍선(내 프롬프트 / claude 응답)
 (최초 1회)        tailnet 상태 표시      스트리밍: 토큰 실시간 append + tool_use 요약칩
                                          하단: 입력창 · [plan 토글] · [전송]/[취소]
                                          완료: 배지(✅/❌/무변경) + [git 변경요약] 펼침
```

**상태 관리:** 데스크톱 `run.ts`의 `appendChunk`·`verdictLabel` 등 **순수 로직을 mobile로 이식**(같은 TS). WS 이벤트 타입(`token/tool_use/done/error`)에 맞춰 리듀서 확장. 연결 상태 `disconnected → connecting → live`, 포그라운드 복귀·네트워크 회복 시 `/stream?offset=N` 자동 재접속·catch-up.

**빌드/배포:** Android는 **EAS build → APK 직접 설치**(스토어·서명 심사 불필요). iOS는 v2.

## 완료 푸시 흐름 (v1, Expo 릴레이 경로 A)

```
1. 폰 앱: Expo Notifications로 push 토큰 획득(FCM) → POST /push/register 로 데몬 저장
2. 실행 중 앱 닫음/백그라운드 → WS 끊김
3. 데몬: run 완료 감지(.done + verdict) →
     - WS 연결 살아있으면: done 이벤트 전송(즉시 표시)
     - 없으면: 등록된 ExpoPushToken으로 완료 푸시(https://exp.host/.../push/send, 인터넷 경유)
4. 폰: 알림 탭 → 앱 열림 → /status/:run_id + /stream?offset 로 결과 복원
```

- 푸시 내용: "✅ <프로젝트> 완료 (변경 3파일)" / "❌ <프로젝트> 실패".
- **인프라(경로 A)**: Firebase 프로젝트 + FCM V1 **서비스계정 JSON을 EAS 크리덴셜에 1회 업로드**. 데몬은 크리덴셜 미보관, ExpoPushToken만 전송. 상주 서버·유료 인프라 없음. 사이드로드 APK도 Google Play Services만 있으면 동작.
- **중복 방지**: WS로 이미 done 받은 run은 데몬이 push 스킵(run별 notified 플래그).
- Android(FCM)만 v1. iOS(APNs)는 v2.

## 잠자기(전원) 관리 — v1

- **실행 중 자동**: 턴이 도는 동안 데몬이 "유휴 sleep 방지" 어서션(`caffeinate`류)을 걸고 완료 시 해제 → 실행 중 Mac이 자버려 끊기는 것 방지.
- **폰 토글**: `/awake {on}` → 데몬이 장기 어서션 유지/해제. 외출 전·카페에서 켜두면 그동안 안 잠듦.
- 한계 명시: 노트북은 배터리 소모, 클램셸+무전원이면 결국 잠듦. **자는 Mac을 깨우는 WoL은 v2**(Tailscale WoL은 상시 LAN 노드·"네트워크 접근 시 깨우기"·하드웨어 조건 필요, 데스크톱 Mac+이더넷에서 신뢰도 높음).

## 에러 처리 & 엣지케이스

- **tailnet 미연결**: "Mac 연결 안 됨(Tailscale 확인)" 배너 + 타임아웃 후 재시도.
- **Mac 잠자기**: launchd 데몬은 깨어날 때 재개. 폰 요청은 못 깨우므로(WoL 비목표) "Mac 응답 없음" → 사용자가 깨우면 자동 재접속.
- **실행 중 새 턴 요청**: 409 → "실행 중, 취소 후 재시도".
- **WS 끊김 중 완료**: done을 로그에 append → 재접속 `?offset` 복원 + 푸시 통지(중복은 notified 플래그로 스킵).
- **토큰 무효/revoke**: 401 → 폰이 페어링 화면으로 유도.
- **stream-json 부분 줄**: 버퍼링, 깨진 줄 skip + error 이벤트.
- **포트 선점 충돌**: 단일 인스턴스 가드 — 이미 바인드면 신규 서버 미기동 + 로깅.

## 테스트 전략

- **awb-core**: 기존 Rust `#[test]` 유지(scan/runner/runlog/lock). lib 추출 후에도 통과 = 회귀 안전망.
- **awb-server**:
  - 유닛 — 페어링 코드 검증·토큰 해시·stream-json 파서(JSONL→이벤트)·단일 인스턴스 가드.
  - 통합 — 가짜 claude(stream-json 출력 스크립트)로 `/chat`→WS 스트림→`/status` 왕복, `/cancel` 실제 PGID kill, 재접속 offset catch-up.
  - 푸시 — Expo API를 목 엔드포인트로 대체해 "WS 없을 때만 발송·notified 중복 스킵" 검증.
- **mobile**: `run.ts` 이식 로직 vitest(appendChunk·verdictLabel·WS 이벤트 리듀서). UI는 v1 수동 스모크(EAS APK 실기기).

## v1 비목표 (→ v2)

iOS 앱(APNs) · Wake-on-LAN(자는 Mac 깨우기) · 다중 동시 실행/턴 큐잉 · 대화 히스토리 영구 저장·검색 · 파일 diff **뷰어**(요약만 v1) · 여러 사용자 협업.
