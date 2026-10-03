<script lang="ts">
  import { onMount } from 'svelte';
  import Icon from './components/Icon.svelte';
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
  import ErrorMessage from './components/ErrorMessage.svelte';
  import DifferenceChecker from './components/DifferenceChecker.svelte';
  import ResizeGrip from './components/ResizeGrip.svelte';
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
  let comparisonOpen = $state(false);
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
      if (error.code !== 'busy') {
        try {
          await refresh();
        } catch {}
      }
    } finally {
      busy = false;
    }
  }
  async function compare(open: boolean) {
    try {
      await call('set_comparison_open', { open });
      comparisonOpen = open;
    } catch (e) {
      error = errorOf(e);
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
      if (comparisonOpen) void compare(false);
      else void hide();
    }
  }
</script>

<svelte:window onkeydown={key} />
<div class="app-shell" class:expanded={comparisonOpen}>
  {#if comparisonOpen}<DifferenceChecker onclose={() => compare(false)} />{/if}
  <main class="flyout">
    <header class="app-header" data-tauri-drag-region>
      <div class="wordmark" data-tauri-drag-region>
        <Icon name="sync" size={16} />
        <h1 data-tauri-drag-region>lazysync</h1>
        {#if preview}<span class="badge">Preview</span>{/if}
      </div>
      <div class="header-actions">
        <button
          class="icon-button"
          onclick={() => (panel = 'settings')}
          aria-label="Settings"
          disabled={operating || comparisonOpen}
          ><Icon name="settings" size={17} /></button
        ><button
          class="icon-button"
          onclick={() => hide()}
          aria-label="Hide lazysync"><Icon name="close" size={17} /></button
        >
      </div>
    </header>
    <div class="content">
      <button
        class="repository-selector"
        onclick={() => (panel = 'repositories')}
        disabled={operating || status?.recovery || comparisonOpen}
        ><span
          ><small>Repository</small><strong
            >{active?.remote.fullName.split('/').pop() ??
              'Choose a repository'}</strong
          ></span
        ><Icon name="down" size={17} /></button
      >
      {#if active && settings}<p class="repository-owner">
          {active.remote.owner} / {active.remote.private ? 'Private' : 'Public'}
        </p>
        <div class="meta">
          <span><Icon name="branch" size={13} />{active.branch}</span><span
            ><Icon name="device" size={13} />{settings.deviceName}</span
          >
        </div>{/if}
      {#if status && active}<StatusIndicator {status} />
        <div class="last-sync">
          <span>Last synced</span><time title={status.lastSync ?? ''}
            >{relative(status.lastSync)}</time
          >
        </div>
        <div class="change-counts">
          <span><b>{status.changes.added}</b> added</span><span
            ><b>{status.changes.modified}</b> edited</span
          ><span><b>{status.changes.deleted}</b> removed</span>
        </div>
        <div class="navigation-rows">
          <button
            onclick={() => compare(!comparisonOpen)}
            disabled={operating || status.recovery}
            aria-expanded={comparisonOpen}
            ><Icon name="difference" size={17} /><span>Difference checker</span
            ><Icon name="right" size={15} /></button
          >
          <button
            onclick={() => (panel = 'history')}
            disabled={operating || comparisonOpen}
            ><Icon name="history" size={17} /><span>History</span><Icon
              name="right"
              size={15}
            /></button
          >
        </div>
        <div class="automation-row">
          <span
            >Automatic sync<small
              >{status.recovery
                ? 'Paused for recovery'
                : settings?.automation === 'folderIdle'
                  ? 'On folder idle'
                  : settings?.automation === 'afterChanges'
                    ? 'After file changes'
                    : 'Off'}</small
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
          <div class="intro-icon"><Icon name="branch" size={26} /></div>
          <h2>Keep your files in sync.</h2>
          <p>Choose a repository to get started.</p>
          <button class="text-button" onclick={() => (panel = 'create')}
            >Create a private repository <Icon
              name="external"
              size={14}
            /></button
          >
        </div>{:else if !error}<p role="status">Loading lazysync…</p>{/if}
      <ErrorMessage error={error ?? status?.error ?? null} />
    </div>
    <div class="bottom-actions">
      {#if status?.recovery}<button
          class="primary sync-button"
          onclick={() => (panel = 'recovery')}
          disabled={operating}
          ><Icon name="sync" size={19} />Review recovery</button
        >{:else if active}<button
          class="primary sync-button"
          onclick={sync}
          disabled={operating || comparisonOpen}
          ><Icon
            name="sync"
            size={19}
            class={operating ? 'spin' : ''}
          />{operating ? 'Syncing…' : 'Sync Now'}</button
        >{:else}<button
          class="primary sync-button"
          onclick={() => (panel = 'settings')}
          ><Icon name="external" size={18} />Connect GitHub</button
        >{/if}
    </div>
  </main>
  <ResizeGrip />
</div>
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
