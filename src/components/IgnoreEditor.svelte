<script lang="ts">
  import { IconFolder, IconFile, IconArrowLeft } from '@tabler/icons-svelte';
  import { call, errorOf } from '../lib/api';
  import { ignorePattern } from '../lib/comparison';
  import type { IgnoreSettings, FolderEntry, AppError } from '../lib/models';
  import ErrorMessage from './ErrorMessage.svelte';
  let {
    folder = null,
    draft = false,
    patterns = $bindable<string[]>([]),
    disabled = false,
  }: {
    folder?: string | null;
    draft?: boolean;
    patterns?: string[];
    disabled?: boolean;
  } = $props();
  let settings = $state<IgnoreSettings | null>(null);
  let entries = $state<FolderEntry[]>([]);
  let directory = $state('');
  let text = $state('');
  let busy = $state(false);
  let error = $state<AppError | null>(null);
  let saved = $state(false);
  let generation = 0;
  const presets = [
    { label: 'Dependencies', rules: ['node_modules/', '.venv/', 'vendor/'] },
    { label: 'Build output', rules: ['dist/', 'build/', 'target/'] },
    { label: 'Secrets', rules: ['.env', '.env.*', '*.pem', '*.key'] },
    { label: 'System files', rules: ['.DS_Store', 'Thumbs.db'] },
  ];
  function change(value: string) {
    text = value;
    patterns = value.split('\n').filter((s) => s.length);
    saved = false;
  }
  async function load(path = '') {
    const id = ++generation;
    busy = true;
    error = null;
    try {
      const [next, files] = await Promise.all([
        call('get_ignore_settings', { folder }),
        call('get_folder_entries', { folder, directory: path }),
      ]);
      if (id !== generation) return;
      settings = next;
      entries = files;
      directory = path;
      if (!draft) change(next.patterns.join('\n'));
      else text = patterns.join('\n');
    } catch (e) {
      if (id === generation) error = errorOf(e);
    } finally {
      if (id === generation) busy = false;
    }
  }
  $effect(() => {
    const selectedFolder = folder;
    if (!draft || selectedFolder) void load();
    else {
      generation++;
      settings = null;
      entries = [];
      text = patterns.join('\n');
    }
  });
  async function browse(path: string) {
    busy = true;
    error = null;
    try {
      entries = await call('get_folder_entries', { folder, directory: path });
      directory = path;
    } catch (e) {
      error = errorOf(e);
    } finally {
      busy = false;
    }
  }
  function toggle(entry: FolderEntry) {
    const rule = ignorePattern(entry.path, entry.directory);
    change(
      (patterns.includes(rule)
        ? patterns.filter((p) => p !== rule)
        : [...patterns, rule]
      ).join('\n'),
    );
  }
  async function save() {
    if (!settings) return;
    busy = true;
    error = null;
    try {
      settings = await call('save_ignore_settings', {
        patterns,
        revision: settings.revision,
      });
      saved = true;
    } catch (e) {
      error = errorOf(e);
    } finally {
      busy = false;
    }
  }
</script>

<div class="ignore-editor">
  <p class="hint">
    Choose what stays on this device. Rules are saved to <code>.gitignore</code> and
    take effect on your next sync.
  </p>
  <ErrorMessage {error} />
  {#if error}<button
      type="button"
      class="text-button"
      onclick={() => load()}
      disabled={busy}>Reload ignore settings</button
    >{/if}
  <div class="ignore-presets">
    {#each presets as preset}<button
        type="button"
        disabled={disabled || busy}
        onclick={() =>
          change([...new Set([...patterns, ...preset.rules])].join('\n'))}
        >{preset.label}</button
      >{/each}
  </div>
  {#if settings}<div class="ignore-browser">
      <div class="ignore-browser-header">
        <span>{directory || 'Files & folders'}</span>{#if directory}<button
            type="button"
            class="icon-button"
            aria-label="Up one folder"
            disabled={busy}
            onclick={() =>
              browse(
                directory.includes('/')
                  ? directory.slice(0, directory.lastIndexOf('/'))
                  : '',
              )}><IconArrowLeft size={14} /></button
          >{/if}
      </div>
      {#each entries as entry}<div class="ignore-entry">
          <input
            type="checkbox"
            aria-label="Ignore {entry.path}"
            checked={patterns.includes(
              ignorePattern(entry.path, entry.directory),
            )}
            disabled={disabled || busy}
            onchange={() => toggle(entry)}
          />
          {#if entry.directory}<button
              type="button"
              class="ignore-folder"
              onclick={() => browse(entry.path)}
              disabled={busy}
              ><IconFolder size={14} /><span
                >{entry.path.split('/').pop()}/</span
              ></button
            >{:else}<span class="ignore-file"
              ><IconFile size={14} />{entry.path.split('/').pop()}</span
            >{/if}
          {#if entry.tracked}<small
              title="Git already tracks this path. Ignore rules do not remove tracked files."
              >Tracked</small
            >{:else if entry.ignored}<small>Ignored</small>{/if}
        </div>{/each}
      {#if !entries.length}<p class="hint">This folder is empty.</p>{/if}
      {#if entries.length === 500}<p class="hint">
          First 500 entries shown. Use patterns for other paths.
        </p>{/if}
    </div>{:else if draft && !folder}<p class="hint">
      Choose a local folder to browse its files.
    </p>{/if}
  <label for="ignore-patterns"
    >Ignore patterns <span class="muted">· one per line</span></label
  >
  <textarea
    id="ignore-patterns"
    rows="5"
    value={text}
    oninput={(e) => change(e.currentTarget.value)}
    disabled={disabled || busy}
    placeholder="node_modules/&#10;*.log&#10;/private-notes/"
    spellcheck="false"></textarea>
  <p class="hint">
    Already tracked files stay tracked. These controls do not delete or untrack
    files.
  </p>
  {#if settings?.existing}<details class="existing-ignore">
      <summary>Existing rules · preserved</summary>
      <pre>{settings.existing}</pre>
    </details>{/if}
  {#if !draft}<button
      type="button"
      class="primary full"
      onclick={save}
      disabled={disabled || busy || !settings}
      >{busy ? 'Saving…' : 'Save ignore rules'}</button
    >{/if}
  {#if saved}<p class="hint" role="status">Ignore rules saved.</p>{/if}
</div>
