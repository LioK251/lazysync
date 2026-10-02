<script lang="ts">
  import { onMount } from 'svelte';
  import { IconGitCommit, IconChevronRight } from '@tabler/icons-svelte';
  import { call, errorOf } from '../lib/api';
  import { relative } from '../lib/format';
  import type { Commit, AppError } from '../lib/models';
  import ErrorMessage from './ErrorMessage.svelte';
  import DiffViewer from './DiffViewer.svelte';
  let commits = $state<Commit[]>([]);
  let page = $state(0);
  let loading = $state(false);
  let error = $state<AppError | null>(null);
  let selected = $state<Commit | null>(null);
  async function load(p: number) {
    loading = true;
    error = null;
    try {
      commits = await call('get_history', { page: p });
      page = p;
    } catch (e) {
      error = errorOf(e);
    } finally {
      loading = false;
    }
  }
  onMount(() => {
    void load(0);
  });
</script>

<ErrorMessage {error} />
{#if selected}<button class="text-button" onclick={() => (selected = null)}
    >← All commits</button
  >
  <h3>{selected.summary}</h3>
  <p class="muted mono">{selected.oid.slice(0, 7)} · {selected.author}</p>
  <DiffViewer oid={selected.oid} />
{:else}<p class="muted">Commit history on the selected branch.</p>
  {#if loading}<p role="status">
      Loading history…
    </p>{:else if !commits.length}<div class="empty">
      <h3>No commits yet</h3>
      <p>Your first Sync Now will create a commit.</p>
    </div>{/if}
  {#each commits as commit}<button
      class="commit-row"
      onclick={() => (selected = commit)}
      ><IconGitCommit size={18} /><span
        ><strong>{commit.summary}</strong><small
          >{commit.oid.slice(0, 7)} · {relative(commit.timestamp)}</small
        ></span
      ><IconChevronRight size={16} /></button
    >{/each}
  <div class="pagination">
    <button disabled={page === 0 || loading} onclick={() => load(page - 1)}
      >Previous</button
    ><span>Page {page + 1}</span><button
      disabled={commits.length < 20 || loading}
      onclick={() => load(page + 1)}>Next</button
    >
  </div>
{/if}
