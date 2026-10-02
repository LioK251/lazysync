<script lang="ts">
  import { onMount } from 'svelte';
  import { call, errorOf } from '../lib/api';
  import type { AppError, FileDiff } from '../lib/models';
  import ErrorMessage from './ErrorMessage.svelte';
  let { oid = null }: { oid?: string | null } = $props();
  let files = $state<FileDiff[]>([]);
  let error = $state<AppError | null>(null);
  let loading = $state(true);
  onMount(async () => {
    try {
      files = await call('get_diffs', { oid });
    } catch (e) {
      error = errorOf(e);
    } finally {
      loading = false;
    }
  });
</script>

<ErrorMessage {error} />
{#if loading}<p class="muted" role="status">
    Loading changes…
  </p>{:else if !files.length}<div class="empty">
    <h3>No pending changes</h3>
    <p>Your working folder matches the last commit.</p>
  </div>{/if}
{#each files as file}<details class="file-diff" open={files.length === 1}>
    <summary
      >{file.path}{#if file.binary}<span class="badge">Binary</span
        >{/if}</summary
    >
    {#if file.binary}<p class="muted">
        Binary file · no text preview
      </p>{:else}<pre
        aria-label="Read-only diff for {file.path}">{#each file.patch.split('\n') as line}<span
            class:added={line.startsWith('+')}
            class:removed={line.startsWith('-')}>{line}{'\n'}</span
          >{/each}</pre>{/if}
    {#if file.truncated}<p class="muted">
        Preview limited to 64 KiB. Open your Git client for the full diff.
      </p>{/if}
  </details>{/each}
{#if files.length === 200}<p class="muted">Showing the first 200 files.</p>{/if}
