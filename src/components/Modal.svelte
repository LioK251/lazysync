<script lang="ts">
  import { onMount } from 'svelte';
  import Icon from './Icon.svelte';
  import type { Snippet } from 'svelte';
  import { call } from '../lib/api';
  let {
    title,
    onclose,
    children,
  }: { title: string; onclose: () => void; children: Snippet } = $props();
  let dialog: HTMLDialogElement;
  onMount(() => {
    const prior = document.activeElement as HTMLElement | null;
    dialog.showModal();
    void call('set_dialog_open', { open: true }).catch(() => {});
    return () => {
      void call('set_dialog_open', { open: false }).catch(() => {});
      prior?.focus();
    };
  });
</script>

<dialog
  bind:this={dialog!}
  oncancel={(e) => {
    e.preventDefault();
    onclose();
  }}
  aria-label={title}
>
  <header class="panel-header" data-tauri-drag-region>
    <button class="icon-button" onclick={onclose} aria-label="Back"
      ><Icon name="back" size={18} /></button
    >
    <h2 data-tauri-drag-region>{title}</h2>
  </header>
  <div class="panel-body">{@render children()}</div>
</dialog>
