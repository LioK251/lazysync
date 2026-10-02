<script lang="ts">
  import Icon from './Icon.svelte';
  import { call, chooseFolder, errorOf } from '../lib/api';
  import type { Settings, AppError } from '../lib/models';
  import ErrorMessage from './ErrorMessage.svelte';
  import IgnoreEditor from './IgnoreEditor.svelte';
  let {
    settings,
    onchange,
  }: { settings: Settings; onchange: (s: Settings) => void } = $props();
  let name = $state('');
  let description = $state('');
  let folder = $state('');
  let loading = $state(false);
  let error = $state<AppError | null>(null);
  let ignorePatterns = $state<string[]>([]);
  $effect(() => {
    if (settings.pendingSetup) {
      name = settings.pendingSetup.name;
      description = settings.pendingSetup.description;
      folder = settings.pendingSetup.folder;
      ignorePatterns = settings.pendingSetup.ignorePatterns;
    }
  });
  async function create(e: SubmitEvent) {
    e.preventDefault();
    loading = true;
    error = null;
    try {
      onchange(
        await call('create_repository', {
          name,
          description,
          folder,
          ignorePatterns,
        }),
      );
    } catch (e) {
      error = errorOf(e);
      try {
        settings = await call('get_settings');
      } catch {}
    } finally {
      loading = false;
    }
  }
</script>

<h3><Icon name="lock" size={16} /> Private repository</h3>
<p class="muted">
  Created on your personal GitHub account. Your files upload when you select
  Sync Now.
</p>
<ErrorMessage {error} />
{#if settings.pendingSetup}<div class="notice">
    Unfinished setup is saved. Resume to attach the same repository.
  </div>{/if}
<form onsubmit={create}>
  <label for="repo-name">Repository name</label><input
    id="repo-name"
    required
    maxlength="100"
    pattern="[A-Za-z0-9._-]+"
    bind:value={name}
    disabled={!!settings.pendingSetup}
    placeholder="my-project"
  />
  <label for="repo-description"
    >Description <span class="muted">(optional)</span></label
  ><input
    id="repo-description"
    bind:value={description}
    maxlength="350"
    placeholder="What lives in this folder?"
  />
  <label for="new-folder">Local folder</label>
  <div class="folder-input">
    <input
      id="new-folder"
      required
      bind:value={folder}
      disabled={!!settings.pendingSetup}
      placeholder="Choose your project folder"
    /><button
      type="button"
      class="icon-button"
      disabled={!!settings.pendingSetup}
      aria-label="Choose local folder"
      onclick={async () => {
        const p = await chooseFolder();
        if (p) folder = p;
      }}><Icon name="folder" size={18} /></button
    >
  </div>
  <p class="hint">
    Existing history is preserved. A conflicting origin will pause setup.
  </p>
  <details class="ignore-section">
    <summary>Ignored files & folders</summary><IgnoreEditor
      folder={folder || null}
      draft
      bind:patterns={ignorePatterns}
      disabled={loading || !!settings.pendingSetup}
    />
  </details>
  <button class="primary full" disabled={loading}
    >{loading
      ? 'Setting up…'
      : settings.pendingSetup
        ? 'Resume setup'
        : 'Create private repository'}</button
  >
</form>
{#if settings.pendingSetup}<button
    class="text-button full"
    disabled={loading}
    onclick={async () => {
      try {
        onchange(await call('abandon_setup'));
      } catch (e) {
        error = errorOf(e);
      }
    }}>Clear saved setup · keep remote repository</button
  >{/if}
