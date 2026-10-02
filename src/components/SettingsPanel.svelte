<script lang="ts">
  import { IconBrandGithub } from '@tabler/icons-svelte';
  import { call, errorOf } from '../lib/api';
  import type { Settings, Automation, AppError } from '../lib/models';
  import ErrorMessage from './ErrorMessage.svelte';
  import IgnoreEditor from './IgnoreEditor.svelte';
  let {
    settings,
    recovery,
    onchange,
  }: {
    settings: Settings;
    recovery: boolean;
    onchange: (s: Settings) => void;
  } = $props();
  let token = $state('');
  let deviceName = $state('');
  let commitName = $state('');
  let commitEmail = $state('');
  let automation = $state<Automation>('off');
  let loading = $state(false);
  let error = $state<AppError | null>(null);
  let identity = $state('');
  $effect(() => {
    deviceName = settings.deviceName;
    commitName = settings.commitName;
    commitEmail = settings.commitEmail;
    automation = settings.automation;
  });
  async function auth(e: SubmitEvent) {
    e.preventDefault();
    loading = true;
    error = null;
    try {
      const who = await call('authenticate', { token });
      token = '';
      identity = who.login;
      const next = await call('get_settings');
      commitName = next.commitName;
      commitEmail = next.commitEmail;
      settings = next;
    } catch (e) {
      error = errorOf(e);
    } finally {
      loading = false;
    }
  }
  async function save(e: SubmitEvent) {
    e.preventDefault();
    loading = true;
    error = null;
    try {
      onchange(
        await call('save_settings', {
          deviceName,
          commitName,
          commitEmail,
          automation,
        }),
      );
    } catch (e) {
      error = errorOf(e);
    } finally {
      loading = false;
    }
  }
</script>

<ErrorMessage {error} />
{#if settings.activeId}<details class="ignore-section repository-settings">
    <summary>Repository settings · ignored files</summary><IgnoreEditor
      disabled={loading || recovery}
    />
  </details>{/if}
<h3 class="section-label">
  <IconBrandGithub size={18} /> GitHub authentication
</h3>
<p class="muted">
  Tokens are stored in your operating system's credential vault.
</p>
<form onsubmit={auth}>
  <label for="token">Personal access token</label><input
    id="token"
    type="password"
    autocomplete="off"
    spellcheck="false"
    bind:value={token}
    required
    placeholder="Paste a classic or fine-grained PAT"
  /><button class="full" disabled={loading || !token}
    >{loading ? 'Validating…' : 'Connect / replace token'}</button
  >
</form>
{#if identity}<p class="green" role="status">Connected as {identity}</p>{/if}
<details class="permissions">
  <summary>Which permissions do I need?</summary>
  <p>Classic PAT: <code>repo</code> scope for private repositories.</p>
  <p>
    Fine-grained PAT: select your repositories; grant Contents read/write and
    Metadata read. Creating a private repository requires Administration
    read/write on your personal account. A new repository may require updating
    the token's repository selection.
  </p>
  <a href="https://github.com/settings/tokens" target="_blank" rel="noreferrer"
    >Manage tokens on GitHub ↗</a
  >
</details>
<form onsubmit={save}>
  <label for="device">Device name</label><input
    id="device"
    bind:value={deviceName}
    required
    disabled={recovery}
  />
  <label for="commit-name">Commit author</label><input
    id="commit-name"
    bind:value={commitName}
    required
    disabled={recovery}
  />
  <label for="commit-email">Commit email</label><input
    id="commit-email"
    bind:value={commitEmail}
    type="email"
    required
    disabled={recovery}
  />
  <label for="automation">Automatic sync</label><select
    id="automation"
    bind:value={automation}
    disabled={recovery}
    ><option value="off">Off</option><option value="afterChanges"
      >After file changes · 2 seconds</option
    ><option value="folderIdle">On folder idle · 30 seconds</option></select
  >
  <p class="hint">
    Only the active folder is watched. Recovery pauses automation.
  </p>
  <button class="primary full" disabled={loading || recovery}
    >Save settings</button
  >
</form>
{#if settings.activeId}<button
    class="full"
    disabled={loading || recovery}
    onclick={async () => {
      loading = true;
      try {
        onchange(await call('reconfirm_branch'));
      } catch (e) {
        error = errorOf(e);
      } finally {
        loading = false;
      }
    }}>Reconfirm current branch</button
  >{/if}
