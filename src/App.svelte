<script lang="ts">
  import { onMount } from 'svelte';
  import {
    IconRefresh,
    IconSettings,
    IconChevronDown,
    IconGitBranch,
    IconDeviceDesktop,
    IconHistory,
    IconArrowUpRight,
    IconX,
  } from '@tabler/icons-svelte';
  import { call, errorOf, onStatus, hide, preview } from './lib/api';
  import { acceptSnapshot, relative } from './lib/format';
  import type { Settings, StatusSnapshot, AppError } from './lib/models';
  import Modal from './components/Modal.svelte';
  import RepositoryPicker from './components/RepositoryPicker.svelte';
  import CreateRepositoryModal from './components/CreateRepositoryModal.svelte';
  import SettingsPanel from './components/SettingsPanel.svelte';
  import HistoryPanel from './components/HistoryPanel.svelte';
  import DiffViewer from './components/DiffViewer.svelte';
  import ConflictResolver from './components/ConflictResolver.svelte';
  import StatusIndicator from './components/StatusIndicator.svelte';
  import DiffSummary from './components/DiffSummary.svelte';
  import ErrorMessage from './components/ErrorMessage.svelte';
  let settings = $state<Settings | null>(null);
  let status = $state<StatusSnapshot | null>(null);
  let panel = $state<
    | 'repositories'
    | 'create'
    | 'settings'
    | 'history'
    | 'diff'
    | 'recovery'
    | null
  >(null);
  let busy = $state(false);
  let error = $state<AppError | null>(null);
  let active = $derived(
    settings?.repositories.find((r) => r.remote.id === settings?.activeId),
  );
  let title = $derived(
    (
      {
        repositories: 'Repositories',
        create: 'Create private repository',
        settings: 'Settings',
        history: 'Commit history',
        diff: 'Pending changes',
        recovery: 'Recover sync',
      } as const
    )[panel ?? 'settings'],
  );
  let operating = $derived(
    busy || (!!status && !['idle', 'paused'].includes(status.phase)),
  );
  async function refresh() {
    settings = await call('get_settings');
    status = await call('get_status');
  }
  async function changed(next: Settings) {
    settings = next;
    status = await call('get_status');
    panel = null;
    error = null;
  }
  async function sync() {
    busy = true;
    error = null;
    try {
      status = await call('sync_now');
      await refresh();
    } catch (e) {
      error = errorOf(e);
      try {
        await refresh();
      } catch {}
    } finally {
      busy = false;
    }
  }
  onMount(() => {
    let disposed = false;
    let unsubscribe = () => {};
    void (async () => {
      try {
        unsubscribe = await onStatus((next) => {
          if (settings && acceptSnapshot(status, next, settings.activeId))
            status = next;
        });
        if (disposed) {
          unsubscribe();
          return;
        }
        await refresh();
      } catch (e) {
        error = errorOf(e);
      }
    })();
    return () => {
      disposed = true;
      unsubscribe();
    };
  });
  function key(e: KeyboardEvent) {
    if (e.key === 'Escape' && !panel) {
      void hide();
    }
  }
</script>

<svelte:window onkeydown={key} />
<main class="flyout">
  <header class="app-header">
    <div class="wordmark">
      <IconRefresh size={19} stroke={2.3} />
      <h1>lazysync</h1>
      {#if preview}<span class="badge">Preview</span>{/if}
    </div>
    <div class="header-actions">
      <button
        class="icon-button"
        onclick={() => (panel = 'settings')}
        aria-label="Settings"
        disabled={operating}><IconSettings size={18} /></button
      ><button
        class="icon-button"
        onclick={() => hide()}
        aria-label="Hide lazysync"><IconX size={17} /></button
      >
    </div>
  </header>
  <div class="content">
    <button
      class="repository-selector"
      onclick={() => (panel = 'repositories')}
      disabled={operating || status?.recovery}
      ><span
        ><small>REPOSITORY</small><strong
          >{active?.remote.fullName ?? 'Choose a repository'}</strong
        ></span
      ><IconChevronDown size={17} /></button
    >
    {#if active && settings}<div class="meta">
        <span><IconDeviceDesktop size={14} />{settings.deviceName}</span><span
          ><IconGitBranch size={14} />{active.branch}</span
        >
      </div>{/if}
    {#if status && active}<StatusIndicator {status} />
      <div class="last-sync">
        <span>Last successful sync</span><time title={status.lastSync ?? ''}
          >{relative(status.lastSync)}</time
        >
      </div>
      <DiffSummary
        counts={status.changes}
        onopen={() => (panel = 'diff')}
        disabled={operating}
      />
      <div class="automation-row">
        <span
          >Automatic sync<small
            >{status.recovery
              ? 'Paused for recovery'
              : settings?.automation === 'folderIdle'
                ? 'On folder idle'
                : settings?.automation === 'afterChanges'
                  ? 'After file changes'
                  : 'You decide when to sync'}</small
          ></span
        ><button
          class="toggle"
          role="switch"
          aria-checked={settings?.automation !== 'off'}
          aria-label="Automatic sync"
          disabled={operating || status.recovery || !settings}
          onclick={async () => {
            if (!settings) return;
            busy = true;
            try {
              settings = await call('save_settings', {
                deviceName: settings.deviceName,
                commitName: settings.commitName,
                commitEmail: settings.commitEmail,
                automation:
                  settings.automation === 'off' ? 'afterChanges' : 'off',
              });
            } catch (e) {
              error = errorOf(e);
            } finally {
              busy = false;
            }
          }}><span></span></button
        >
      </div>
    {:else if settings}<div class="empty welcome">
        <div class="intro-icon"><IconGitBranch size={26} /></div>
        <h2>Your folder. In sync.</h2>
        <p>Connect a GitHub repository and keep your files together.</p>
        <button class="text-button" onclick={() => (panel = 'create')}
          >Create a private repository <IconArrowUpRight size={14} /></button
        >
      </div>{:else if !error}<p role="status">Loading lazysync…</p>{/if}
    <ErrorMessage error={error ?? status?.error ?? null} />
  </div>
  <div class="bottom-actions">
    {#if status?.recovery}<button
        class="primary sync-button"
        onclick={() => (panel = 'recovery')}
        disabled={operating}><IconRefresh size={19} />Review recovery</button
      >{:else if active}<button
        class="primary sync-button"
        onclick={sync}
        disabled={operating}
        ><IconRefresh size={19} class={operating ? 'spin' : ''} />{operating
          ? 'Syncing…'
          : 'Sync Now'}</button
      >{:else}<button
        class="primary sync-button"
        onclick={() => (panel = 'settings')}
        ><IconArrowUpRight size={18} />Connect GitHub</button
      >{/if}
  </div>
  <footer>
    <button
      class="text-button"
      onclick={() => (panel = 'history')}
      disabled={!active || operating}><IconHistory size={15} />History</button
    ><button class="text-button muted" onclick={() => call('quit')}>Quit</button
    >
  </footer>
</main>
{#if panel}<Modal
    {title}
    onclose={() => {
      panel = null;
      void refresh().catch((e) => {
        error = errorOf(e);
      });
    }}
  >
    {#if panel === 'repositories' && settings}<RepositoryPicker
        {settings}
        onchange={changed}
        oncreate={() => (panel = 'create')}
      />
    {:else if panel === 'create' && settings}<CreateRepositoryModal
        {settings}
        onchange={changed}
      />
    {:else if panel === 'settings'}{#if settings}<SettingsPanel
          {settings}
          recovery={status?.recovery ?? false}
          onchange={changed}
        />{:else}<p class="muted">
          Run the native desktop app to configure lazysync.
        </p>{/if}
    {:else if panel === 'history'}<HistoryPanel
      />{:else if panel === 'diff'}<DiffViewer
      />{:else if panel === 'recovery'}<ConflictResolver
        oncomplete={async (next) => {
          status = next;
          settings = await call('get_settings');
          panel = null;
          error = null;
        }}
      />{/if}
  </Modal>{/if}
