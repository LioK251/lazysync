<script lang="ts">
  import { onMount } from 'svelte';
  import Icon from './Icon.svelte';
  import { call, chooseFolder, errorOf } from '../lib/api';
  import type { Settings, RemoteRepository, AppError } from '../lib/models';
  import ErrorMessage from './ErrorMessage.svelte';
  let {
    settings,
    onchange,
    oncreate,
  }: {
    settings: Settings;
    onchange: (s: Settings) => void;
    oncreate: () => void;
  } = $props();
  let repositories = $state<RemoteRepository[]>([]);
  let page = $state(1);
  let query = $state('');
  let loading = $state(false);
  let more = $state(true);
  let selected = $state<RemoteRepository | null>(null);
  let folder = $state('');
  let error = $state<AppError | null>(null);
  let filtered = $derived(
    repositories.filter((r) =>
      r.fullName.toLowerCase().includes(query.toLowerCase()),
    ),
  );
  async function load() {
    loading = true;
    error = null;
    try {
      const next = await call('list_repositories', { page });
      repositories = [
        ...repositories,
        ...next.filter((n) => !repositories.some((r) => r.id === n.id)),
      ];
      more = next.length === 30;
      page++;
    } catch (e) {
      error = errorOf(e);
    } finally {
      loading = false;
    }
  }
  async function select(id: string) {
    loading = true;
    error = null;
    try {
      onchange(await call('select_repository', { id }));
    } catch (e) {
      error = errorOf(e);
    } finally {
      loading = false;
    }
  }
  async function connect(clone: boolean) {
    if (!selected) return;
    loading = true;
    error = null;
    try {
      onchange(
        clone
          ? await call('clone_repository', { remote: selected, folder })
          : await call('connect_repository', { remote: selected, folder }),
      );
    } catch (e) {
      error = errorOf(e);
    } finally {
      loading = false;
    }
  }
  onMount(() => {
    void load();
  });
</script>

<ErrorMessage {error} />
{#if selected}<button class="text-button" onclick={() => (selected = null)}
    >← Accessible repositories</button
  >
  <h3>{selected.fullName}</h3>
  <p class="muted">
    Connect a checkout with matching origin, or clone into an empty folder.
  </p>
  <label for="checkout-folder">Local folder</label>
  <div class="folder-input">
    <input
      id="checkout-folder"
      bind:value={folder}
      placeholder="Choose a folder"
    /><button
      class="icon-button"
      aria-label="Choose local folder"
      onclick={async () => {
        const p = await chooseFolder();
        if (p) folder = p;
      }}><Icon name="folder" size={18} /></button
    >
  </div>
  <button
    class="primary full"
    disabled={loading || !folder}
    onclick={() => connect(true)}
    >{loading ? 'Connecting…' : 'Clone to folder'}</button
  ><button
    class="full"
    disabled={loading || !folder}
    onclick={() => connect(false)}>Connect existing checkout</button
  >
{:else}
  {#if settings.repositories.length}<h3 class="section-label">
      Remembered folders
    </h3>
    {#each settings.repositories as mapping}<button
        class="repo-row"
        disabled={loading}
        onclick={() => select(mapping.remote.id)}
        ><Icon name="folder" size={18} /><span
          ><strong>{mapping.remote.fullName}</strong><small
            title={mapping.folder}>{mapping.folder}</small
          ></span
        >{#if mapping.remote.id === settings.activeId}<span class="badge"
            >Active</span
          >{/if}</button
      >{/each}{/if}
  <h3 class="section-label">On GitHub</h3>
  <div class="search">
    <Icon name="search" size={16} /><input
      aria-label="Search loaded repositories"
      bind:value={query}
      placeholder="Search loaded repositories…"
    />
  </div>
  <p class="hint">Search filters loaded pages. Load more to search further.</p>
  {#each filtered as repo}<button
      class="repo-row"
      disabled={loading || !repo.writable}
      onclick={() => {
        selected = repo;
        folder = '';
      }}
      ><Icon name="lock" size={17} /><span
        ><strong>{repo.fullName}</strong><small
          >{repo.owner} · {repo.private ? 'Private' : 'Public'} · {repo.writable
            ? 'Write access'
            : 'Read only'}</small
        ></span
      ></button
    >{/each}
  {#if loading}<p role="status" class="muted">
      Loading repositories…
    </p>{:else if !filtered.length}<p class="muted">
      No matching repositories on these pages.
    </p>{/if}
  {#if more}<button class="full" disabled={loading} onclick={load}
      >Load more repositories</button
    >{/if}<button class="primary full" disabled={loading} onclick={oncreate}
    >Create private repository</button
  >
{/if}
