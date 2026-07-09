<script lang="ts">
  import { call } from "./api";
  import { appendChunk, verdictLabel, type RunState } from "./run";
  import type { Project, RunHandle, LogChunk, RunStatus } from "./types";

  let { project, claudeBin }: { project: Project; claudeBin: string | null } = $props();

  const settingsPath = "~/.claude/worker-settings.json";
  const runsDir = "~/.claude/.awb-runs";
  const POLL_MS = 1500;

  let prompt = $state("");
  let plan = $state(false);
  let running = $state(false);
  let error = $state("");
  let handle = $state<RunHandle | null>(null);
  let log = $state<RunState>({ text: "", offset: 0 });
  let status = $state<RunStatus | null>(null);
  let timer: ReturnType<typeof setInterval> | undefined;

  function stopPolling() {
    if (timer !== undefined) {
      clearInterval(timer);
      timer = undefined;
    }
  }

  async function poll() {
    if (!handle) return;
    try {
      const chunk = await call<LogChunk>("read_log", { log: handle.log, offset: log.offset });
      log = appendChunk(log, chunk);
      if (chunk.done) {
        stopPolling();
        status = await call<RunStatus>("run_status", { log: handle.log, workdir: project.path });
        running = false;
      }
    } catch (err) {
      error = `로그 조회 실패: ${err}`;
      stopPolling();
      running = false;
    }
  }

  async function run() {
    if (!claudeBin) {
      error = "claude 실행 파일 경로를 찾을 수 없습니다 (프리플라이트 확인).";
      return;
    }
    error = "";
    status = null;
    log = { text: "", offset: 0 };
    running = true;
    try {
      handle = await call<RunHandle>("start_run", {
        claude_bin: claudeBin,
        workdir: project.path,
        settings: settingsPath,
        plan,
        prompt,
        runs_dir: runsDir,
      });
      stopPolling();
      timer = setInterval(poll, POLL_MS);
    } catch (err) {
      error = `실행 시작 실패: ${err}`;
      running = false;
    }
  }

  async function cancel() {
    if (!handle) return;
    try {
      await call<boolean>("cancel_run", { pgid: handle.pgid, workdir: project.path });
    } catch (err) {
      error = `취소 실패: ${err}`;
    } finally {
      stopPolling();
      running = false;
    }
  }

  $effect(() => {
    return () => stopPolling();
  });
</script>

<div class="console">
  <h2>{project.name}</h2>

  <textarea
    class="prompt"
    placeholder="프롬프트를 입력하세요..."
    bind:value={prompt}
    disabled={running}
  ></textarea>

  <div class="controls">
    <label>
      <input type="checkbox" bind:checked={plan} disabled={running} />
      plan 모드
    </label>
    <button onclick={run} disabled={running || !prompt.trim()}>실행</button>
    <button onclick={cancel} disabled={!running}>취소</button>
  </div>

  {#if error}
    <p class="error">{error}</p>
  {/if}

  {#if status}
    <p class="verdict">{verdictLabel(status.verdict)} (변경 파일 {status.changed_files}개)</p>
  {:else if running}
    <p class="verdict">{verdictLabel("running")}</p>
  {/if}

  <pre class="log">{log.text}</pre>
</div>

<style>
  .console {
    width: 100%;
    margin-top: 1rem;
  }
  .prompt {
    width: 100%;
    box-sizing: border-box;
    min-height: 5rem;
    padding: 0.5rem;
    font-family: inherit;
  }
  .controls {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    margin: 0.5rem 0;
  }
  .verdict {
    font-weight: 600;
  }
  .error {
    color: #b00020;
  }
  .log {
    background: #111;
    color: #ddd;
    padding: 0.75rem;
    max-height: 20rem;
    overflow: auto;
    white-space: pre-wrap;
    word-break: break-word;
  }
</style>
