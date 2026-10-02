<script lang="ts">
  import type { DiffCounts } from '../lib/generated/DiffCounts';
  let {
    counts,
    onopen,
    disabled = false,
  }: { counts: DiffCounts; onopen: () => void; disabled?: boolean } = $props();
</script>

<button
  class="diff-summary"
  onclick={onopen}
  {disabled}
  aria-label="View pending changes"
>
  <span><b class="green">+{counts.added}</b><small>added</small></span><span
    ><b class="amber">~{counts.modified}</b><small>modified</small></span
  ><span><b class="red">−{counts.deleted}</b><small>deleted</small></span>
</button>
