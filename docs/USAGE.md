# ai-workbench 사용법 (v0 — Plan 1+2)

현재 브랜치 `plan2-execution` 기준. 프로젝트를 조회하고, 프로젝트별로 프롬프트를 실행해
진행·결과를 보는 데스크톱 앱입니다. (실시간 토큰 스트림·멀티턴 대화·diff 뷰·크로스-PC는 이후 Plan 3+/v2.)

---

## 0. 사전 준비 (한 번만)

| 필요 | 확인/설치 |
|------|-----------|
| Rust (rustup) | `cargo --version` (없으면 `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh -s -- -y`) |
| Node ≥ 20 | `node -v` (현재 v22 확인됨) |
| claude CLI | `claude --version` — 앱이 로그인셸 PATH에서 자동 탐색(현재 `~/.local/bin/claude`) |
| 권한 프로필 | `~/.claude/worker-settings.json` 존재해야 함(이미 있음). 실행 시 이 deny 목록 적용 |
| git-crypt | 트랜스크립트 조회용. 잠겨 있으면 앱이 배너로 알려줌 |
| 프로젝트 위치 | `~/bitbucket/*` (현재 repo들), `~/github/*` (향후). git repo + origin 있어야 목록에 뜸 |

## 1. 실행

```bash
cd ~/github/ai-workbench
git checkout plan2-execution      # 테스트할 브랜치
npm install                        # 최초 1회
npm run tauri dev                  # 앱 창 실행 (첫 실행은 Rust 빌드로 수 분)
```
창이 뜨면 상단에 **프리플라이트 배너**(문제 있으면 빨간색), 아래에 **프로젝트 목록**이 보입니다.

> 빌드만 확인하려면: `npm run build` (프론트) + `cd src-tauri && cargo build` (백엔드).

## 2. 프로젝트 목록 화면

- **소스**: `~/bitbucket`·`~/github` 안의, `git` 최상위 폴더이면서 `origin` 리모트가 있는 repo만.
- **정렬/검색**: 최근 활동순 + 검색창(이름 부분일치) + 핀.
- **worklog 배지**: `~/.claude/worklog/<분기>/<프로젝트>.md` 가 있으면 ⬜/🔄/✅ 개수 표시.
- **프리플라이트 배너**: claude 미발견 / roots 없음 / worker-settings 없음 / git-crypt 잠김 중
  실패 항목만 빨간 배너로. (모두 정상이면 배너 없음.)

## 3. 프롬프트 실행 (핵심)

1. 목록에서 **프로젝트 선택** → 콘솔 화면.
2. 프롬프트 입력 + **plan 토글**:
   - **끔(기본)** = 실제 작업(파일 편집·명령 실행 허용, `worker-settings.json` deny만 차단).
   - **켬(plan)** = 읽기전용/분석만(`--permission-mode plan`), 변경 없음 — 안전 탐색용.
3. **실행** 버튼 → 진행이 **로그 tail(1~2초)** 로 흐릅니다.
4. 완료 시 **배지**:
   - `✅ 완료` (변경 있음) / `✅ 완료(변경 없음)` / `❌ 실패(exit N)`
   - 변경 파일 수·exit code 표시.
5. **취소** 버튼: 실행 중 프로세스 그룹을 종료(SIGTERM→SIGKILL)하고 락 해제.

## 4. 동작 방식 (참고)

- 실행은 프로젝트 폴더에서 `claude -p <프롬프트> --settings ~/.claude/worker-settings.json` (plan이면 `--permission-mode plan`)을 **detached**로 돌립니다.
- 로그: `~/.claude/.awb-runs/<timestamp>.log`, 완료 마커 `<log>.done`.
- **공유 실행 락**: `~/.claude/.run-locks/<키>` — 같은 프로젝트에 앱과 자율 폴러(`project-poll`)가
  **동시에 claude를 띄우지 못하게** 막습니다. (실행 중이면 "이미 실행 중" 표시.)
- 완료 시 좀비 프로세스 회수 + 락 자동 해제.

## 5. 안전장치

- **worker-settings deny**: 권한상승(sudo/su)·시스템전원(shutdown/reboot)·이름기반 프로세스몰살(killall/pkill)·git파괴(force push/reset --hard 등) 차단. 그 외 명령은 허용(당신이 정한 정책).
- **plan 토글**로 "그냥 물어보기/분석"만 안전하게 가능.
- 실제 파일 변경이 일어나므로(비-plan 모드), 실행 후 `git diff`로 확인 권장. (앱 내 diff 뷰는 Plan 3.)

## 6. 문제 해결

| 증상 | 조치 |
|------|------|
| 프리플라이트 "claude 미발견" | 터미널에서 `which claude` 확인; 앱을 터미널에서 `npm run tauri dev`로 실행하면 PATH 상속 |
| 목록이 빔 | `~/bitbucket`에 git repo(+origin) 있는지 확인. roots는 현재 `~/bitbucket`,`~/github` 고정(설정 UI는 Plan 4) |
| git-crypt 잠김 배너 | `cd ~/.claude && gpg -d keys/git-crypt-key.gpg > /tmp/k && git-crypt unlock /tmp/k && rm /tmp/k` |
| 실행이 멈춘 듯 | `~/.claude/.awb-runs/`의 최신 `.log` 확인. `.done` 없으면 진행 중, 있으면 완료(exit 기록) |
| "이미 실행 중" | 다른 실행/폴러가 락 보유. 끝나면 자동 해제. 강제 정리: `rm -rf ~/.claude/.run-locks/<키>` |

## 7. 현재 범위 (v0)

- ✅ 로컬 프로젝트 조회 + 프리플라이트 + worklog 배지 (Plan 1)
- ✅ 프롬프트 실행 + 로그 tail + 완료 판정 + 취소 + 공유 락 + plan 토글 (Plan 2)
- ⬜ (Plan 3) 앱 내 `git diff` 뷰·베이스라인 롤백·권한 하드닝·출력 redaction
- ⬜ (v1) 실시간 토큰 스트림·`--resume` 멀티턴 대화·프롬프트 프리셋
- ⬜ (v2) 크로스-PC 프로젝트 조회·원격 실행

## 8. 피드백

테스트 중 이상/원하는 개선은 Slack `#claude-ai-workbench` 에 남겨주세요 — 폴링으로 반영합니다.
