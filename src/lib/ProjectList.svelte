<script lang="ts">
  import { call } from "./api";
  import { filterProjects } from "./scan";
  import type { Project, Badge } from "./types";

  let { roots, onSelect }: { roots: string[]; onSelect?: (p: Project) => void } = $props();

  let projects = $state<Project[]>([]);
  let badges = $state<Record<string, Badge | null>>({});
  let query = $state("");
  let pinned = $state<string[]>([]);
  let loading = $state(true);
  let error = $state("");

  function badgeText(b: Badge | null | undefined): string {
    if (!b) return "";
    if (b.doing > 0) return "🔄";
    if (b.todo > 0) return "⬜";
    if (b.done > 0) return "✅";
    return "";
  }

  function togglePin(name: string) {
    pinned = pinned.includes(name) ? pinned.filter((n) => n !== name) : [...pinned, name];
  }

  let filtered = $derived.by(() => {
    const f = filterProjects(projects, query);
    return [...f].sort((a, b) => {
      const ap = pinned.includes(a.name) ? 1 : 0;
      const bp = pinned.includes(b.name) ? 1 : 0;
      if (ap !== bp) return bp - ap;
      return 0; // 프로젝트 배열은 이미 last_activity 내림차순(백엔드 정렬)
    });
  });

  async function load() {
    loading = true;
    error = "";
    try {
      projects = await call<Project[]>("list_projects", { roots });
      const entries = await Promise.all(
        projects.map(async (p) => [p.name, await call<Badge | null>("worklog_badge", { name: p.name })] as const)
      );
      badges = Object.fromEntries(entries);
    } catch (err) {
      error = `프로젝트 목록을 불러오지 못했습니다: ${err}`;
    } finally {
      loading = false;
    }
  }

  load();
</script>

<div class="project-list">
  <input
    class="search"
    type="text"
    placeholder="프로젝트 검색..."
    bind:value={query}
  />

  {#if loading}
    <p>불러오는 중...</p>
  {:else if error}
    <p class="error">{error}</p>
  {:else if filtered.length === 0}
    <p>일치하는 프로젝트가 없습니다.</p>
  {:else}
    <ul>
      {#each filtered as p (p.name)}
        <li>
          <button class="pin" onclick={() => togglePin(p.name)} aria-label="핀 고정 토글">
            {pinned.includes(p.name) ? "📌" : "☆"}
          </button>
          <span class="badge">{badgeText(badges[p.name])}</span>
          <button class="name-btn" onclick={() => onSelect?.(p)}>
            <span class="name">{p.name}</span>
            <span class="path">{p.path}</span>
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .project-list {
    width: 100%;
  }
  .search {
    width: 100%;
    box-sizing: border-box;
    padding: 0.5rem;
    margin-bottom: 0.75rem;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  li {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.4rem 0.2rem;
    border-bottom: 1px solid #e0e0e0;
  }
  .pin {
    background: none;
    border: none;
    cursor: pointer;
    font-size: 1rem;
  }
  .name-btn {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    flex: 1;
    min-width: 0;
    background: none;
    border: none;
    cursor: pointer;
    padding: 0;
    text-align: left;
    font: inherit;
  }
  .name {
    font-weight: 600;
  }
  .path {
    color: #888;
    font-size: 0.85rem;
    margin-left: auto;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .error {
    color: #b00020;
  }
</style>
