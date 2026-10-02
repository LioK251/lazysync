<script lang="ts">
  import { onMount } from 'svelte';
  import { call, errorOf } from '../lib/api';
  import type { Conflict, AppError, StatusSnapshot } from '../lib/models';
  import ErrorMessage from './ErrorMessage.svelte';
  let { oncomplete }: { oncomplete: (s: StatusSnapshot) => void } = $props();
  let files = $state<Conflict[]>([]);
  let loading = $state(false);
  let error = $state<AppError | null>(null);
  let loaded = $state(false);
  let manual = $state(false);
  onMount(async () => {
    try {
      files = await call('get_conflicts');
    } catch (e) {
      error = errorOf(e);
    } finally {
      loaded = true;
    }
  });
  async function resolve(path: string, choice: 'local' | 'remote' | 'edited') {
    loading = true;
    error = null;
    try {
      files = await call('resolve_conflict', { path, choice });
    } catch (e) {
      error = errorOf(e);
    } finally {
      loading = false;
    }
  }
  async function proceed() {
    loading = true;
    error = null;
    try {
      oncomplete(await call('continue_sync'));
    } catch (e) {
      error = errorOf(e);
      files = await call('get_conflicts');
    } finally {
      loading = false;
    }
  }
</script>

<p class="muted">
  Automation is paused. Both versions are saved before you choose.
</p>
<ErrorMessage {error} />
{#if !loaded}<p role="status">Reading recovery…</p>{/if}
{#each files as file}<div class="conflict-file">
    <h3>{file.path}</h3>
    {#if file.resolved}<p class="green">Resolved</p>{:else}
      <div class="conflict-preview">
        <h4>
          Local {#if file.local === null}· deleted{/if}
        </h4>
        <pre>{file.local ?? 'File deleted locally'}</pre>
      </div>
      <div class="conflict-preview">
        <h4>
          Remote {#if file.remote === null}· deleted{/if}
        </h4>
        <pre>{file.remote ?? 'File deleted remotely'}</pre>
      </div>
      {#if file.binary}<p class="hint">
          Binary conflict. Choosing a side preserves its complete bytes.
        </p>{/if}{#if file.truncated}<p class="hint">
          Previews are limited to 64 KiB; full versions are saved.
        </p>{/if}
      <div class="two-buttons">
        <button disabled={loading} onclick={() => resolve(file.path, 'local')}
          >Keep Local</button
        ><button disabled={loading} onclick={() => resolve(file.path, 'remote')}
          >Accept Remote</button
        >
      </div>
      <button
        class="full"
        disabled={loading}
        onclick={async () => {
          try {
            await call('open_conflict_editor', { path: file.path });
          } catch (e) {
            error = errorOf(e);
          }
        }}>Open in Editor</button
      ><button
        class="text-button full"
        disabled={loading}
        onclick={() => resolve(file.path, 'edited')}
        >Mark editor changes resolved</button
      >
    {/if}
  </div>{/each}
{#if loaded && !files.length}<div class="notice">
    An interrupted sync may be ready to retry. Continue Sync resumes verified
    steps. If local state is uncertain, finish recovery in your Git client.
  </div>{/if}
<button
  class="primary full"
  disabled={loading || !loaded || files.some((f) => !f.resolved)}
  onclick={proceed}>{loading ? 'Continuing…' : 'Continue Sync'}</button
>
<details class="manual-recovery">
  <summary>Finish recovery in a Git client</summary>
  <p>
    Preserve your files, inspect the saved stash and <code
      >refs/lazysync/backups</code
    >, and finish any Git operation first. The app retains all recovery backups
    and stashes.
  </p>
  <label class="check-label"
    ><input type="checkbox" bind:checked={manual} /> I have reconciled my files and
    saved stash.</label
  ><button
    class="full"
    disabled={!manual || loading}
    onclick={async () => {
      loading = true;
      try {
        oncomplete(await call('finish_recovery'));
      } catch (e) {
        error = errorOf(e);
      } finally {
        loading = false;
      }
    }}>Finish Recovery · turn automation off</button
  >
</details>
