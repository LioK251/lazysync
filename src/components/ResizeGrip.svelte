<script lang="ts">
  import { isTauri } from '@tauri-apps/api/core';
  import { call } from '../lib/api';
  let error = $state('');
  function failed() {
    error = 'Could not resize the window. Try dragging a window edge.';
  }
  async function drag(event: PointerEvent) {
    if (event.button !== 0 || !isTauri()) return;
    (event.currentTarget as HTMLButtonElement).focus();
    event.preventDefault();
    error = '';
    try {
      const { getCurrentWindow } = await import('@tauri-apps/api/window');
      await getCurrentWindow().startResizeDragging('SouthEast');
    } catch {
      failed();
    }
  }
  async function key(event: KeyboardEvent) {
    if (
      !['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown'].includes(event.key)
    )
      return;
    event.preventDefault();
    const step = event.shiftKey ? 64 : 16;
    const width =
      event.key === 'ArrowRight' ? step : event.key === 'ArrowLeft' ? -step : 0;
    const height =
      event.key === 'ArrowDown' ? step : event.key === 'ArrowUp' ? -step : 0;
    error = '';
    try {
      await call('resize_window_by', { width, height });
    } catch {
      failed();
    }
  }
</script>

{#if error}<p class="resize-error" role="status">{error}</p>{/if}
<button
  type="button"
  class="resize-grip"
  aria-label="Resize window"
  title="Drag to resize. Arrow keys resize; Shift resizes faster."
  onpointerdown={drag}
  onkeydown={key}
>
  <svg
    width="16"
    height="16"
    viewBox="0 0 16 16"
    fill="none"
    stroke="currentColor"
    stroke-width="1"
    aria-hidden="true"
  >
    <path d="M4 13 13 4M8 13l5-5M12 13l1-1" />
  </svg>
</button>
