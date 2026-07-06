<script lang="ts">
  import { call } from "./api";
  import type { Preflight, Check } from "./types";

  let { roots, claudeOverride }: { roots: string[]; claudeOverride?: string } = $props();

  let preflight = $state<Preflight | null>(null);
  let error = $state("");

  const hints: Record<string, string> = {
    claude: "claude CLI가 PATH에 없습니다. 설치 후 PATH에 추가하거나 경로를 직접 지정하세요.",
    roots: "유효한 project_roots가 없습니다. 설정에서 실제 존재하는 디렉터리를 지정하세요.",
    worker_settings: "~/.claude/worker-settings.json 이 없습니다. 워커 설정 파일을 생성하세요.",
    git_crypt: "트랜스크립트가 git-crypt로 잠겨 있습니다. `git-crypt unlock` 을 실행하세요.",
  };

  function hintFor(c: Check): string {
    return hints[c.id] ?? "";
  }

  let failing = $derived(preflight?.checks.filter((c) => !c.ok) ?? []);

  async function load() {
    try {
      preflight = await call<Preflight>("preflight", { roots, claude_override: claudeOverride ?? null });
    } catch (err) {
      error = `프리플라이트 확인 실패: ${err}`;
    }
  }

  load();
</script>

{#if error}
  <div class="banner">
    <p>{error}</p>
  </div>
{:else if failing.length > 0}
  <div class="banner">
    <ul>
      {#each failing as c (c.id)}
        <li>
          <strong>{c.id}</strong>: {c.detail}
          {#if hintFor(c)}
            <span class="hint"> — {hintFor(c)}</span>
          {/if}
        </li>
      {/each}
    </ul>
  </div>
{/if}

<style>
  .banner {
    background: #fdecea;
    border: 1px solid #f5c2c0;
    color: #611a15;
    border-radius: 4px;
    padding: 0.6rem 0.8rem;
    margin-bottom: 0.75rem;
  }
  .banner ul {
    margin: 0;
    padding-left: 1.2rem;
  }
  .hint {
    color: #7a3b34;
    font-size: 0.9em;
  }
</style>
