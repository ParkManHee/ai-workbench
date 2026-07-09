<script lang="ts">
  import { call } from "./api";
  import type { Project } from "./types";

  interface SessionInfo { session_id: string; updated: number; preview: string; answer_preview: string; count: number; active: boolean; waiting: boolean }
  interface TxMsg { role: string; text: string; tools: string[]; tool_details: string[] }
  interface Page { messages: TxMsg[]; prev: number | null; next: number; active: boolean }

  let { project }: { project: Project } = $props();

  let sessions = $state<SessionInfo[]>([]);
  let selected = $state<string | null>(null);
  let messages = $state<TxMsg[]>([]);
  let prev = $state<number | null>(null);
  let next = 0; // 라이브 폴 오프셋(라인 수) — 렌더에 안 쓰여 반응성 불필요
  let error = $state("");
  let openTools = $state<Set<number>>(new Set());
  let listEl = $state<HTMLDivElement>();
  let timer: ReturnType<typeof setInterval> | undefined;

  function stopPoll() {
    if (timer !== undefined) { clearInterval(timer); timer = undefined; }
  }

  async function loadSessions() {
    try {
      sessions = await call<SessionInfo[]>("list_sessions", { project_path: project.path });
    } catch (e) {
      error = `세션 목록 조회 실패: ${e}`;
    }
  }

  async function open(sid: string) {
    stopPoll();
    selected = sid; error = ""; openTools = new Set();
    try {
      const p = await call<Page>("transcript_page", { project_path: project.path, session_id: sid, until: null });
      messages = p.messages; prev = p.prev; next = p.next;
      scrollBottom();
      if (p.active) startPoll();
    } catch (e) {
      error = `대화 조회 실패: ${e}`;
    }
  }

  // 활성 세션(90초 내 갱신)은 2초 폴링으로 새 메시지를 이어 붙인다 — 모바일과 동일한 증분 방식.
  function startPoll() {
    timer = setInterval(async () => {
      if (!selected) return;
      try {
        const r = await call<{ messages: TxMsg[]; next: number; active: boolean }>(
          "transcript_from", { project_path: project.path, session_id: selected, from: next });
        next = r.next;
        if (r.messages.length > 0) { messages = [...messages, ...r.messages]; scrollBottom(); }
        if (!r.active) stopPoll();
      } catch {
        // 일시 오류 — 다음 틱에 재시도
      }
    }, 2000);
  }

  async function loadOlder() {
    if (!selected || prev == null) return;
    try {
      const p = await call<Page>("transcript_page", { project_path: project.path, session_id: selected, until: prev });
      prev = p.prev;
      openTools = new Set(); // index 기반 펼침 상태는 시프트되므로 리셋
      messages = [...p.messages, ...messages];
    } catch (e) {
      error = `이전 대화 조회 실패: ${e}`;
    }
  }

  function scrollBottom() {
    setTimeout(() => { if (listEl) listEl.scrollTop = listEl.scrollHeight; }, 30);
  }
  function toggle(i: number) {
    const n = new Set(openTools);
    if (n.has(i)) n.delete(i); else n.add(i);
    openTools = n;
  }
  function fmt(ts: number) {
    return new Date(ts * 1000).toLocaleString("ko-KR", { month: "numeric", day: "numeric", hour: "2-digit", minute: "2-digit" });
  }

  $effect(() => {
    void project.path; // 프로젝트 변경 시 초기화 후 재조회
    selected = null; messages = []; prev = null; error = "";
    loadSessions();
    return () => stopPoll();
  });
</script>

<div class="transcript">
  <div class="sessions">
    <div class="sessions-head">
      <span>세션</span>
      <button class="small" onclick={loadSessions}>새로고침</button>
    </div>
    {#each sessions as s (s.session_id)}
      <button class="session" class:selected={selected === s.session_id} onclick={() => open(s.session_id)}>
        <span class="preview">{s.active ? "🟢 " : s.waiting ? "🔴 " : ""}{s.preview || "(프롬프트 없음)"}</span>
        {#if s.answer_preview}<span class="answer">↳ {s.answer_preview}</span>{/if}
        <span class="meta">{fmt(s.updated)} · {s.count}개</span>
      </button>
    {:else}
      <p class="empty">세션이 없습니다.</p>
    {/each}
  </div>

  <div class="chat">
    {#if error}<p class="error">{error}</p>{/if}
    {#if selected}
      <div class="msgs" bind:this={listEl}>
        {#if prev != null}
          <button class="small more" onclick={loadOlder}>이전 대화 더 보기</button>
        {/if}
        {#each messages as m, i}
          <div class="row {m.role === 'user' ? 'user' : 'assistant'}">
            <div class="bubble">
              {#if m.tools && m.tools.length > 0}
                <button class="tools" onclick={() => toggle(i)}>🔧 작업 {m.tools.length} {openTools.has(i) ? "▲" : "▼"}</button>
                {#if openTools.has(i)}
                  {#each (m.tool_details?.length ? m.tool_details : m.tools) as d}
                    <pre class="tool-detail">{d}</pre>
                  {/each}
                {/if}
              {/if}
              {#if m.text.trim()}<div class="text">{m.text}</div>{/if}
            </div>
          </div>
        {/each}
      </div>
    {:else}
      <p class="empty">왼쪽에서 세션을 선택하면 대화가 표시됩니다. 모바일에서 진행한 내용도 같은 세션에 기록됩니다.</p>
    {/if}
  </div>
</div>

<style>
  .transcript {
    display: flex;
    gap: 0.75rem;
    margin-top: 1rem;
    min-height: 24rem;
  }
  .sessions {
    width: 16rem;
    flex-shrink: 0;
    border: 1px solid #ddd;
    border-radius: 8px;
    overflow-y: auto;
    max-height: 32rem;
  }
  .sessions-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 0.5rem;
    font-weight: 600;
    border-bottom: 1px solid #eee;
  }
  .session {
    display: block;
    width: 100%;
    text-align: left;
    padding: 0.5rem;
    border: none;
    border-bottom: 1px solid #eee;
    background: none;
    cursor: pointer;
  }
  .session.selected { background: #eef4ff; }
  .session .preview {
    display: block;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .session .answer {
    display: block;
    font-size: 0.8rem;
    color: #666;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .session .meta { font-size: 0.75rem; color: #888; }
  .chat { flex: 1; min-width: 0; }
  .msgs {
    border: 1px solid #ddd;
    border-radius: 8px;
    padding: 0.75rem;
    max-height: 32rem;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }
  .row { display: flex; }
  .row.user { justify-content: flex-end; }
  .row.assistant { justify-content: flex-start; }
  .bubble {
    max-width: 80%;
    padding: 0.5rem 0.7rem;
    border-radius: 10px;
    white-space: pre-wrap;
    word-break: break-word;
  }
  .row.user .bubble { background: #dcefff; }
  .row.assistant .bubble { background: #f0f0f0; }
  .tools {
    border: none;
    background: #e6e6e6;
    border-radius: 6px;
    font-size: 0.75rem;
    padding: 0.1rem 0.4rem;
    cursor: pointer;
    margin-bottom: 0.25rem;
  }
  .tool-detail {
    font-size: 0.72rem;
    background: #f7f7f7;
    border-radius: 6px;
    padding: 0.3rem 0.4rem;
    margin: 0 0 0.25rem;
    overflow-x: auto;
  }
  .small {
    font-size: 0.75rem;
    padding: 0.15rem 0.5rem;
  }
  .more { align-self: center; }
  .empty { color: #888; }
  .error { color: #b00020; }
</style>
