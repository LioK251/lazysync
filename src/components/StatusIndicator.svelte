<script lang="ts">
  import {
    IconCircleCheck,
    IconAlertTriangle,
    IconRefresh,
  } from '@tabler/icons-svelte';
  import type { StatusSnapshot } from '../lib/models';
  import { tone } from '../lib/format';
  let { status }: { status: StatusSnapshot } = $props();
  let color = $derived(tone(status));
</script>

<div class="status-block {color}" aria-live="polite" aria-atomic="true">
  <div class="status-icon">
    {#if color === 'green'}<IconCircleCheck
        size={25}
      />{:else if color === 'red'}<IconAlertTriangle
        size={25}
      />{:else}<IconRefresh size={25} />{/if}
  </div>
  <div>
    <strong>{status.label}</strong>
    <p>
      {status.recovery
        ? 'Automation paused · your backups are safe'
        : status.connectivity === 'unavailable'
          ? 'Connection unavailable · changes stay local'
          : status.ahead || status.behind
            ? `${status.ahead} ahead · ${status.behind} behind origin`
            : color === 'green'
              ? 'Your local folder is up to date'
              : 'Local changes are ready to upload'}
    </p>
  </div>
</div>
