<script lang="ts">
  import PreflightBanner from "./lib/PreflightBanner.svelte";
  import ProjectList from "./lib/ProjectList.svelte";

  // 기본 roots — Plan 4에서 설정화 예정
  const roots = ["~/bitbucket", "~/github"];
</script>

<main>
  <h1>ai-workbench</h1>
  <PreflightBanner {roots} />
  <ProjectList {roots} />
</main>
