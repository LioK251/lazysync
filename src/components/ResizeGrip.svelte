<script lang="ts">
  import { isTauri } from '@tauri-apps/api/core';
  import { onDestroy } from 'svelte';
  import { call } from '../lib/api';
  let error = $state('');
  let pointer: { id: number; x: number; y: number } | null = null;
  let queuedWidth = 0;
  let queuedHeight = 0;
  let resizing = false;
  let disposed = false;
  onDestroy(() => {
    disposed = true;
    pointer = null;
    queuedWidth = queuedHeight = 0;
  });
  async function applyDrag() {
    if (resizing) return;
    resizing = true;
    try {
      while (!disposed && (queuedWidth || queuedHeight)) {
        const width = Math.max(-64, Math.min(64, queuedWidth));
        const height = Math.max(-64, Math.min(64, queuedHeight));
        queuedWidth -= width;
        queuedHeight -= height;
        await call('resize_window_by', { width, height });
      }
    } catch {
      pointer = null;
      queuedWidth = queuedHeight = 0;
      failed();
    } finally {
      resizing = false;
    }
  }
  function move(event: PointerEvent) {
    if (pointer?.id !== event.pointerId) return;
    queuedWidth += event.screenX - pointer.x;
    queuedHeight += event.screenY - pointer.y;
    pointer.x = event.screenX;
    pointer.y = event.screenY;
    void applyDrag();
  }
  function stop(event: PointerEvent) {
    if (pointer?.id === event.pointerId) pointer = null;
  }
  function failed() {
    error = 'Could not resize the window. Try dragging a window edge.';
  }
  async function drag(event: PointerEvent) {
    if (event.button !== 0 || !isTauri()) return;
    (event.currentTarget as HTMLButtonElement).focus();
    event.preventDefault();
    error = '';
    if (/Mac/i.test(navigator.platform)) {
      // Tao's native drag_resize_window is unsupported on macOS. Screen
      // coordinates remain stable while the viewport changes during a drag.
      pointer = { id: event.pointerId, x: event.screenX, y: event.screenY };
      (event.currentTarget as HTMLButtonElement).setPointerCapture(
        event.pointerId,
      );
      return;
    }
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
  onpointermove={move}
  onpointerup={stop}
  onpointercancel={stop}
  onlostpointercapture={stop}
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
