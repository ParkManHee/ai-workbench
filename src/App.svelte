<script lang="ts">
  import { call } from "./lib/api";
  import PreflightBanner from "./lib/PreflightBanner.svelte";
  import ProjectList from "./lib/ProjectList.svelte";
  import Console from "./lib/Console.svelte";
  import Transcript from "./lib/Transcript.svelte";
  import type { Preflight, Project } from "./lib/types";

  // 기본 roots — Plan 4에서 설정화 예정
  const roots = ["~/bitbucket", "~/github"];

  let claudeBin = $state<string | null>(null);
  let selected = $state<Project | null>(null);
  let tab = $state<"run" | "chat">("run");

  async function loadClaudeBin() {
    try {
      const pf = await call<Preflight>("preflight", { roots, claude_override: null });
      claudeBin = pf.claude_path;
    } catch {
      claudeBin = null;
    }
  }

  loadClaudeBin();
</script>

<main>
  <h1>ai-workbench</h1>
  <PreflightBanner {roots} />
  <ProjectList {roots} onSelect={(p) => (selected = p)} />
  {#if selected}
    <div class="tabs">
      <button class:active={tab === "run"} onclick={() => (tab = "run")}>실행</button>
      <button class:active={tab === "chat"} onclick={() => (tab = "chat")}>대화 내역</button>
    </div>
    <!-- 탭 전환 시 언마운트하면 실행 폴링이 끊기므로 숨김 처리로 유지 -->
    <div style:display={tab === "run" ? "block" : "none"}>
      <Console project={selected} {claudeBin} />
    </div>
    <div style:display={tab === "chat" ? "block" : "none"}>
      <Transcript project={selected} />
    </div>
  {/if}
</main>

<style>
  .tabs {
    display: flex;
    gap: 0.5rem;
    margin-top: 1rem;
  }
  .tabs button {
    padding: 0.3rem 0.9rem;
    border: 1px solid #ccc;
    border-radius: 8px;
    background: none;
    cursor: pointer;
  }
  .tabs button.active {
    background: #2f6fed;
    color: white;
    border-color: #2f6fed;
  }
</style>
