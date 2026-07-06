<script lang="ts">
  import { call } from "./lib/api";
  import PreflightBanner from "./lib/PreflightBanner.svelte";
  import ProjectList from "./lib/ProjectList.svelte";
  import Console from "./lib/Console.svelte";
  import type { Preflight, Project } from "./lib/types";

  // 기본 roots — Plan 4에서 설정화 예정
  const roots = ["~/bitbucket", "~/github"];

  let claudeBin = $state<string | null>(null);
  let selected = $state<Project | null>(null);

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
    <Console project={selected} {claudeBin} />
  {/if}
</main>
