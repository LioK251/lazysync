<script lang="ts">
  export type IconName =
    | 'sync'
    | 'settings'
    | 'down'
    | 'right'
    | 'branch'
    | 'device'
    | 'history'
    | 'external'
    | 'close'
    | 'difference'
    | 'check'
    | 'warning'
    | 'key'
    | 'lock'
    | 'folder'
    | 'search'
    | 'file'
    | 'cloud'
    | 'back'
    | 'commit';
  let {
    name,
    size = 18,
    class: className = '',
  }: { name: IconName; size?: number; class?: string } = $props();
</script>

<svg
  xmlns="http://www.w3.org/2000/svg"
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="1.4"
  stroke-linecap="round"
  stroke-linejoin="round"
  class={className}
  aria-hidden="true"
>
  {#if name === 'sync'}<path
      d="M18.7 9.3a7 7 0 0 0-12-2.5L4.5 9M4.5 4.5V9H9M5.3 14.7a7 7 0 0 0 12 2.5l2.2-2.2M19.5 19.5V15H15"
    />
  {:else if name === 'settings'}<path d="M3 7h4m4 0h10M3 17h10m4 0h4" /><circle
      cx="9"
      cy="7"
      r="2"
    /><circle cx="15" cy="17" r="2" />
  {:else if name === 'down'}<path d="m7 10 5 5 5-5" />
  {:else if name === 'right'}<path d="m10 7 5 5-5 5" />
  {:else if name === 'branch'}<circle cx="6" cy="5" r="2" /><circle
      cx="6"
      cy="19"
      r="2"
    /><circle cx="18" cy="5" r="2" /><path d="M6 7v10m12-10a7 7 0 0 1-7 7H6" />
  {:else if name === 'device'}<rect
      x="3"
      y="4"
      width="18"
      height="12"
      rx="1.5"
    /><path d="M9 20h6m-3-4v4" />
  {:else if name === 'history'}<path d="M4 9a8 8 0 1 1 0 6M4 4v5h5m3-2v5l3 2" />
  {:else if name === 'external'}<path d="M6 18 18 6M8 6h10v10" />
  {:else if name === 'close'}<path d="m6 6 12 12M6 18 18 6" />
  {:else if name === 'difference'}<rect
      x="3"
      y="4"
      width="7"
      height="16"
      rx="1"
    /><rect x="14" y="4" width="7" height="16" rx="1" /><path
      d="M5 8h3m8 8h3"
    />
  {:else if name === 'check'}<path d="m5 12 4 4L19 6" />
  {:else if name === 'warning'}<circle cx="12" cy="12" r="8" /><path
      d="M12 7v6m0 4h.01"
    />
  {:else if name === 'key'}<circle cx="8" cy="9" r="4" /><path
      d="m11 12 8 8m-3-3 2-2m-5 0 2-2"
    />
  {:else if name === 'lock'}<rect
      x="5"
      y="10"
      width="14"
      height="11"
      rx="1.5"
    /><path d="M8 10V7a4 4 0 0 1 8 0v3" />
  {:else if name === 'folder'}<path
      d="M3 6a1 1 0 0 1 1-1h5l2 3h9a1 1 0 0 1 1 1v10a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1Z"
    />
  {:else if name === 'search'}<circle cx="10.5" cy="10.5" r="6.5" /><path
      d="m15.2 15.2 5 5"
    />
  {:else if name === 'file'}<path d="M14 3H5v18h14V8Zm0 0v5h5" />
  {:else if name === 'cloud'}<path
      d="M7 18a4.5 4.5 0 0 1-1-8.9 6 6 0 0 1 11.7 1.4A3.8 3.8 0 0 1 18 18Z"
    />
  {:else if name === 'back'}<path d="M20 12H4m6-6-6 6 6 6" />
  {:else if name === 'commit'}<circle cx="12" cy="12" r="4" /><path
      d="M3 12h5m8 0h5"
    />{/if}
</svg>
