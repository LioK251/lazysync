<script lang="ts">
  import { onMount } from 'svelte';
  import Icon from './Icon.svelte';
  import { call, errorOf } from '../lib/api';
  import {
    languageFor,
    highlightLines,
    comparisonRows,
  } from '../lib/comparison';
  import type { ComparisonList, FileComparison, AppError } from '../lib/models';
  import ErrorMessage from './ErrorMessage.svelte';
  import { formatBytes, sizeTitle, exceedsLimit } from '../lib/size';
  let { onclose }: { onclose: () => void } = $props();
  let list = $state<ComparisonList | null>(null);
  let file = $state<FileComparison | null>(null);
  let selected = $state('');
  let filter = $state('');
  let loading = $state(false);
  let fileLoading = $state(false);
  let error = $state<AppError | null>(null);
  let request = 0;
  let language = $derived(languageFor(file?.path ?? ''));
  let rows = $derived(
    file && !file.binary
      ? comparisonRows(file.cloud ?? '', file.local ?? '')
      : [],
  );
  let localLines = $derived(highlightLines(file?.local ?? '', language));
  let cloudLines = $derived(highlightLines(file?.cloud ?? '', language));
  let visible = $derived(
    list?.files.filter((f) =>
      f.path.toLowerCase().includes(filter.toLowerCase()),
    ) ?? [],
  );
  async function select(path: string) {
    const id = ++request;
    selected = path;
    file = null;
    fileLoading = true;
    error = null;
    try {
      const result = await call('get_file_comparison', {
        path,
        cloudOid: list?.cloudOid ?? null,
      });
      if (id === request) file = result;
    } catch (e) {
      if (id === request) error = errorOf(e);
    } finally {
      if (id === request) fileLoading = false;
    }
  }
  async function refresh() {
    loading = true;
    error = null;
    file = null;
    list = null;
    ++request;
    try {
      list = await call('get_comparison_files');
      if (list.files.length)
        await select(
          list.files.some((f) => f.path === selected)
            ? selected
            : list.files[0].path,
        );
    } catch (e) {
      error = errorOf(e);
    } finally {
      loading = false;
    }
  }
  onMount(() => {
    void refresh();
    return () => {
      ++request;
    };
  });
</script>

<section class="difference-checker" aria-label="Difference checker">
  <header class="comparison-header">
    <h2 data-tauri-drag-region>Difference checker</h2>
    <div class="header-actions">
      <button
        class="icon-button"
        aria-label="Refresh cloud comparison"
        onclick={refresh}
        disabled={loading}
        ><Icon name="sync" size={16} class={loading ? 'spin' : ''} /></button
      >
      <button
        class="icon-button"
        aria-label="Close difference checker"
        onclick={onclose}><Icon name="close" size={16} /></button
      >
    </div>
  </header>
  <div class="comparison-caption">
    <span>Working folder ↔ GitHub</span><span
      >{list
        ? `Checked ${new Date(list.checkedAt).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}`
        : 'Fetching cloud branch…'}</span
    >
  </div>
  <ErrorMessage {error} />
  {#if error}<button
      class="text-button retry-comparison"
      onclick={refresh}
      disabled={loading}>Retry comparison</button
    >{/if}
  {#if loading}<div class="empty comparison-empty" role="status">
      Fetching the latest cloud files…
    </div>
  {:else if list && !list.files.length}<div class="empty comparison-empty">
      <Icon name="cloud" size={26} />
      <h3>No differences</h3>
      <p>
        Your local files match {list.cloudOid
          ? `origin/${list.branch}`
          : 'the empty cloud branch'}.
      </p>
    </div>
  {:else if list}<div class="comparison-workspace">
      <aside class="comparison-files">
        <label class="sr-only" for="comparison-filter"
          >Search different files</label
        ><input
          id="comparison-filter"
          placeholder="Find a file…"
          bind:value={filter}
        />
        <p class="file-count">{list.files.length} different files</p>
        <div class="file-list">
          {#each visible as item}<button
              class="comparison-file"
              class:selected={selected === item.path}
              onclick={() => select(item.path)}
              aria-pressed={selected === item.path}
              title={item.path}
            >
              <Icon name="file" size={14} /><span
                ><strong>{item.path.split('/').pop()}</strong><small
                  >{item.path.includes('/')
                    ? item.path.slice(0, item.path.lastIndexOf('/'))
                    : item.status === 'localOnly'
                      ? 'Only local'
                      : item.status === 'cloudOnly'
                        ? 'Only cloud'
                        : 'Modified'}</small
                ><small class="comparison-file-size">
                  {#if item.localSize !== null}<span
                      title={sizeTitle(item.localSize)}
                      >Local {formatBytes(item.localSize)}</span
                    >{/if}
                  {#if item.cloudSize !== null}<span
                      title={sizeTitle(item.cloudSize)}
                      >Cloud {formatBytes(item.cloudSize)}</span
                    >{/if}
                </small>{#if exceedsLimit(item.localSize)}<small
                    class="size-limit">Over 100 MiB · ignore before sync</small
                  >{/if}</span
              ><Icon name="right" size={12} />
            </button>{/each}
        </div>
        {#if list.truncated}<p class="hint">First 500 files shown.</p>{/if}
      </aside>
      <div class="comparison-content">
        <div class="file-toolbar">
          <span title={selected}>{selected}</span><span class="badge"
            >{language}</span
          >
        </div>
        <div class="comparison-sides">
          <span
            ><Icon name="device" size={14} />Local
            {#if file}<b class="file-size" title={sizeTitle(file.localSize)}
                >{file.localSize === null
                  ? 'Missing'
                  : formatBytes(file.localSize)}</b
              >{/if}
            <small>Working file</small></span
          ><span
            ><Icon name="cloud" size={14} />Cloud
            {#if file}<b class="file-size" title={sizeTitle(file.cloudSize)}
                >{file.cloudSize === null
                  ? 'Missing'
                  : formatBytes(file.cloudSize)}</b
              >{/if}
            <small
              >{list.cloudOid
                ? `origin/${list.branch} · ${list.cloudOid.slice(0, 7)}`
                : 'Empty branch'}</small
            ></span
          >
        </div>
        {#if fileLoading}<div class="empty" role="status">Loading file…</div>
        {:else if file?.binary}<div class="empty comparison-empty">
            <h3>Binary file</h3>
            <p>
              Text preview is unavailable. {file.local === null
                ? 'This file exists only in the cloud.'
                : file.cloud === null
                  ? 'This file exists only locally.'
                  : 'The local and cloud versions differ.'}
            </p>
          </div>
        {:else if file}
          {#if file.local === null || file.cloud === null}<p
              class="comparison-note"
            >
              {file.local === null
                ? 'Missing locally. The cloud file is shown on the right.'
                : 'Not in the cloud. The local file is shown on the left.'}
            </p>{/if}
          <!-- svelte-ignore a11y_no_noninteractive_tabindex (Scrollable code needs keyboard access.) -->
          <div
            class="code-scroll"
            role="region"
            aria-label="Local and cloud file comparison"
            tabindex="0"
          >
            <div class="code-grid">
              {#each rows as row}<div
                  class="code-row"
                  class:changed={row.changed}
                >
                  <div class="code-cell" class:gap={row.local === null}>
                    <span class="line-number"
                      >{row.local === null ? '' : row.local + 1}</span
                    ><code
                      >{@html row.local === null
                        ? ''
                        : (localLines[row.local] ?? '')}</code
                    >
                  </div>
                  <div class="code-cell" class:gap={row.cloud === null}>
                    <span class="line-number"
                      >{row.cloud === null ? '' : row.cloud + 1}</span
                    ><code
                      >{@html row.cloud === null
                        ? ''
                        : (cloudLines[row.cloud] ?? '')}</code
                    >
                  </div>
                </div>{/each}
            </div>
            {#if !rows.length}<p class="empty">Empty file</p>{/if}
          </div>
          {#if file.truncated}<p class="comparison-note">
              Preview limited to 64 KiB per version. Changes shown apply to this
              preview.
            </p>{/if}
        {/if}
      </div>
    </div>{/if}
  <div class="comparison-footer">
    <span><i></i> Changed lines</span><span>Read-only comparison</span>
  </div>
</section>
