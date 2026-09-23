<script lang="ts">
  let { size = 40, wordmark = true }: { size?: number; wordmark?: boolean } = $props();
</script>

<span class="logo">
  <svg width={size} height={size} viewBox="0 0 1024 1024" aria-hidden="true">
    <rect x="64" y="64" width="896" height="896" rx="208" fill="var(--accent)" />
    <circle cx="512" cy="512" r="236" fill="none" stroke="var(--accent-contrast)" stroke-width="88" />
    <circle cx="512" cy="512" r="74" fill="var(--accent-contrast)" />
  </svg>
  {#if wordmark}<span class="word">opnlocal</span>{/if}
</span>

<style>
  .logo {
    display: inline-flex;
    align-items: center;
    gap: 0.55rem;
  }
  .word {
    font-weight: 700;
    font-size: 1.2rem;
    letter-spacing: -0.02em;
  }
</style>
