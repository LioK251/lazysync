<script lang="ts">
  import type { AppError } from '../lib/models';
  let { error }: { error: AppError | null } = $props();
</script>

{#if error}<div class="error" role="alert">
    <strong>{error.message}</strong>
    <p>{error.action}</p>
  </div>{/if}
