# claude-console — 설계 스펙 (v0)

작성일: 2026-07-06
상태: 설계 승인됨 (구현 계획 대기)
저장소: `ParkManHee/claude-console` (개인 GitHub), 로컬 `~/github/claude-console`

## 1. 목적

로컬 PC의 개발 프로젝트를 조회하고, 프로젝트별로 프롬프트를 입력해 Claude에게 작업시키고,
진행·결과·변경(diff)을 데스크톱 앱에서 확인한다. 기존 `~/.claude` 툴링
(worker-settings 권한 프로필 · `agent-run.sh` 실행 래퍼 · git-crypt 동기화 · worklog)을 재사용한다.

한 줄 정의: **"프로젝트별 Claude 실행 콘솔"** — Slack 채널/CLI 대신 전용 앱 UI.

## 2. 스코프

### v0 (이 스펙의 범위)
- 로컬 PC 전용 (크로스-PC 없음)
- 프로젝트 목록 조회 (설정형 roots, git repo 한정, worklog 오버레이)
- 프로젝트별 프롬프트 실행 (detached `claude -p`, plan/실행 토글)
- 진행 표시 = 로그 tail (1~2초 폴링)
- 완료 다층 판정 + 최종답변 + `git diff --stat` / diff 뷰 + 베이스라인 롤백
- 공유 실행 락 · claude 프리플라이트 · worker-settings 보강

### v1 (이후)
실시간 토큰 스트림(stream-json) · `--resume` 멀티턴 지속대화 · 취소 PGID 에스컬레이션 고도화 · 프롬프트 프리셋

### v2
크로스-PC 조회(타 PC 프로젝트/트랜스크립트) · 원격 실행

### 명시적 비목표 (YAGNI)
- 인앱 HITL 권한 승인 버튼 (plan/실행 2모드로 대체)
- 팀/멀티유저 (개인 1인)
- 크로스-PC 트랜스크립트 원문 조회 (그 PC에서 앱 켜기)

## 3. 아키텍처

Tauri 앱: 얇은 Rust 코어 + 웹 UI(Svelte).

```
┌───────────────────────────────────────────────┐
│ 웹 UI (Svelte)                                  │
│  프로젝트 목록 / 프롬프트·대화 / 진행·diff       │
└───────────────┬───────────────────────────────┘
                │ Tauri command / event
┌───────────────▼───────────────────────────────┐
│ Rust 코어 (얇게)                                │
│  - project_scan   프로젝트 발견/필터/정렬        │
│  - runner         detached 실행 + 로그 tail      │
│  - lock           공유 실행 락(realpath 키)       │
│  - preflight      claude/PATH/git-crypt/roots    │
│  - state          app state 원자 읽기/쓰기        │
│  - gitinfo        diff --stat, 신선도(fetch)     │
└───────────────┬───────────────────────────────┘
                │ subprocess / fs
┌───────────────▼───────────────────────────────┐
│ 기존 자산 (재사용)                               │
│  agent-run.sh · worker-settings.json · sync.sh   │
│  ~/.claude/projects/* (트랜스크립트) · worklog   │
└───────────────────────────────────────────────┘
```

각 유닛의 책임/의존은 §4~§9에 정의.

## 4. 프로젝트 목록 (project_scan)

- **소스**: 설정형 `project_roots` (기본값 `["~/bitbucket", "~/github"]`, app state에 저장). 하드코딩 금지.
  - 실측: 현재 `~/bitbucket`에 repo 52개, `~/github`는 아직 비어있음 → 둘 다 루트로.
- **후보 한정**: 각 후보 폴더가 `git rev-parse --show-toplevel == 그 폴더` **이고** `origin` remote가 있을 때만 프로젝트로 인정. 부모 모노repo(`crocus.git`)·`node_modules`·중첩 repo 제외.
- **정렬/표시**: 최근 활동순(트랜스크립트/커밋 mtime) + 핀 + 퍼지 필터. 비활성 repo는 접힌 "전체(N)" 그룹으로 강등.
- **worklog 오버레이**: `worklog/<YYYY-Qn>/<basename>.md` 규칙 하나로 매핑, ⬜/🔄/✅·최종갱신 배지. 미매칭이면 "로그 없음"으로 일관 표시(목록 안 깨짐). 앱은 worklog를 **읽기만** 한다.

## 5. 실행 & 안전 (runner · lock · preflight)

### 5.1 실행
- 프로젝트 워크트리에서 `claude -p <prompt> --settings <worker-settings 경로>` 를 **detached(setsid, 자체 PGID)** 로 실행.
- 기존 `agent-run.sh` 패턴 재사용: 파일 로그 + 완료 시 `.done` 마커. 앱은 로그를 tail-follow, 앱 재시작 시 실행 중이면 재부착.
- **실행 소유권은 앱이 아니라 detached 프로세스** — 앱 창을 닫아도 작업은 계속되고, 재실행 시 재부착.

### 5.2 공유 실행 락
- `mkdir` 원자 락, **키 = 워크트리 realpath**(심링크 해소, remote/채널명 아님).
- 락 파일에 `{pid, pgid, start_ts, source}` 기록.
- **앱 · `agent-run.sh` · `project-poll.py` launch() 모두**에 동일 락 삽입 → 같은 워크트리에 claude 2개 동시 실행 방지. (앱만 걸면 폴러가 무력화하므로 양측 필수.)

### 5.3 claude 프리플라이트 (시작 시 1회 + 실행 전)
- **PATH**: GUI 앱은 `/opt/homebrew/bin`·`~/.local/bin` 미상속 → 로그인 셸에서 PATH 추출. claude 실제 경로(`~/.local/bin/claude`)를 app state에 **단일 진실원**으로 고정, `agent-run`/폴러/앱이 동일 바이너리 참조.
- `claude --version` 파싱 + 최소버전 확인, `-p` 에코로 인증까지 1회 검사.
- **env 화이트리스트**: PATH/HOME/SHELL/LANG/TERM + keychain 접근 유지, `.slack-*`/AWS/`GH_TOKEN` 등 시크릿 env 제거(차단 아니라 화이트리스트).
- **프리플라이트 체크리스트 화면**: claude버전 / git-crypt unlocked / project_roots 유효 / worker-settings 존재 — 각 항목 통과·실패 + 해결 버튼. 빈 목록이면 "루트 추가" 안내(콜드스타트 빈화면 방지). git-crypt unlock은 안내만(대행 금지).

### 5.4 worker-settings 보강 (별도 프로필, 기존 것 확장)
- deny 추가: `rm -rf`, `git branch -D`, `git reset --hard`, `git checkout --`, `git rebase`, `curl|sh` 등 파괴 벡터. (심층방어 최종 층으로만 취급.)
- **시크릿 접근 차단**: `.env`·`~/.ssh`·`~/.aws`·`.slack-*`·`keys/**`·`*config*.json`·`.mcp.json` 의 **Read 및 Bash `cat`/`grep`** 차단. 파일시스템 순회 진입 자체를 코드레벨 차단, 에러 로그는 basename만.
- **MCP 하드락**: `enableAllProjectMcpServers=false` + 화이트리스트(또는 `--strict-mcp-config`). repo에 `.mcp.json` 있으면 경고 배지.
- plan 모드용 별도 `reader-settings.json`(defaultMode=plan + 파괴계 deny 하드락) 생성. plan은 UI 상태가 아니라 `--permission-mode` 인자로 강제.

### 5.5 완료 다층 판정
- **exit 코드 단독 금지.** `result.subtype` + `is_error` + `permission_denials[]` + 변경 파일 수로 판정.
- UI에 "변경 N / 도구 M / 권한거부 K / 결론 유무". 거부>0 또는 "변경0인데 성공"이면 노란 배너.
- 실패 클래스 분리: 미설치/인증만료(exit≠0) vs "deny로 막힘"(exit0 + denials>0) — 다른 메시지·해결안.

## 6. UI 흐름

1. **프로젝트 목록**: 검색/핀/worklog 배지/최근순. 프리플라이트 실패 시 상단 배너.
2. **프로젝트 선택 → 콘솔**: 프롬프트 입력 + **plan/실행 토글** + 실행 버튼 + 취소 버튼. worklog 패널(🔄/⬜/📌) 읽기전용 병치, "해야할일→프롬프트" 클릭 시에만 주입.
3. **진행**: 로그 tail(1~2초 폴링)로 단계/도구 활동 표시 + 실행 중 배지(출처/시작시각).
4. **완료**: 최종답변 강조 + 완료 다층 판정 배너 + `git diff --stat` 자동 표시. 파일 클릭 시 diff 뷰.
5. **롤백**: 실행 시작 시 HEAD SHA + `git stash create`(미커밋분) 스냅샷. 되돌리기는 **이번 세션 변경분만**(전체 restore 금지), 되돌린 분은 stash 보관. 실행 전 dirty면 커밋/스태시 권고.

## 7. 상태 & 저장 (state)

- 앱 상태: `project_roots`·핀·최근 프롬프트·claude 바이너리 경로·프로젝트별 세션ID 포인터 → `~/.claude/app/<pc>/state.json`, **PC별** 저장 후 git 동기화. **원자 쓰기(temp+rename)**, 단일 라이터(다중 창 대비).
  - (세션ID는 cwd 종속이라 크로스-PC resume 불가 → 포인터는 PC별. v0는 resume 미사용이나 스키마는 예약.)
- 대화 트랜스크립트: 기존 `~/.claude/projects/*`(git-crypt) 그대로. 앱은 렌더/저장 직전 `hooks/redact.sh`로 세척.
- **git-crypt unlock 감지**: 대표 `.jsonl` 첫 16바이트 `GITCRYPT` 매직 검사 → 잠겨 있으면 "언락 필요" 배너(암호문을 대화로 렌더 방지). 앱 산출물 확장자를 `.gitattributes`와 교차검증(state를 `.jsonl`로 저장해 자동암호화되는 함정 방지).

## 8. git / 동기화 통합 (gitinfo)

- **git 조작은 `sync.sh` 경유** — 앱이 직접 pull/commit 금지(`.sync.lock` 공유, 병렬 rebase 충돌 방지).
- 신선도 표시는 `git fetch`만 후 origin 대비 계산("마지막 동기화 N분 전"). 실제 pull은 사용자/idle 시 위임.
- 실행 전 워크트리 dirty 감지 → 커밋/스태시 권고.

## 9. 에러 처리 / 실패 모드

- claude 미설치/PATH 부재 → 프리플라이트에서 조기 차단 + 해결 안내(조용한 무반응 금지).
- 장시간 실행 → 진행 배지 유지, (v1) 타임아웃 + PGID kill.
- 앱 크래시/종료 → detached 실행 지속, 재시작 시 `.done`/`.run` 마커로 진행중·완료·고아 구분해 재부착.
- 로그 tail 중 잘린 JSON/부분 출력 → v0는 텍스트 tail이라 영향 적음(스트림 파서는 v1).
- 보고/표시 시 항상 redact 적용(env dump/curl 헤더/파일내용).

## 10. 스택 / 산출물

- Tauri(Rust 코어) + Svelte(웹 UI) + 기존 셸 스크립트 재사용.
- 저장소: `ParkManHee/claude-console`.
- 기존 `~/.claude` 자산 변경 최소화: worker-settings 보강, agent-run.sh/project-poll.py에 공유 락 추가는 별도 작업으로 조율.

## 11. 오픈 항목 (구현 계획에서 확정)

- 프론트 프레임워크 최종(Svelte 가정, React 가능).
- worker-settings vs reader-settings 파일 분리 방식.
- 앱↔agent-run.sh 로그/락 규약(경로·포맷) 정확한 계약.
- 앱 자체 repo의 GitHub 생성/CI 여부.

## 12. 테스트 전략

- Rust 코어 유닛: project_scan(가짜 디렉토리 트리), lock(동시성), preflight(가짜 PATH), 완료 판정(가짜 result JSON).
- 통합: 실제 소규모 repo에서 plan 모드 1회·실행 모드 1회 → diff/완료판정 검증.
- 안전 회귀: deny 목록(시크릿 read·rm -rf·MCP) 실제 차단 확인.
