<script lang="ts">
  import { t } from "../lib/strings";
  import type { Fit } from "../lib/bindings/Fit";

  // Estimated fit only; speed is never shown here (only measured results show speed).
  let { fit }: { fit: Fit } = $props();
</script>

<p class="fit {fit.label}">
  <span class="dot" aria-hidden="true"></span>
  <span>
    <strong>{t.fit.label[fit.label]}</strong>
    {#if fit.label === "too_big"}
      · {t.fit.shortBy(fit.short_by_bytes)}
    {:else}
      · {t.fit.placement[fit.placement]}
    {/if}
    {#if fit.label === "tight"}<br /><span class="muted small">{t.fit.tightHint}</span>{/if}
    {#if fit.close_other_apps}<br /><span class="muted small">{t.fit.closeApps}</span>{/if}
  </span>
</p>

<style>
  .fit {
    display: flex;
    gap: 0.55rem;
    align-items: flex-start;
    margin: 0;
  }
  .dot {
    flex: none;
    width: 10px;
    height: 10px;
    border-radius: 50%;
    margin-top: 0.45em;
  }
  .comfortable .dot {
    background: var(--good);
  }
  .tight .dot {
    background: var(--warn);
  }
  .too_big .dot {
    background: transparent;
    border: 2px solid var(--text-2);
  }
  .comfortable strong {
    color: var(--good);
  }
  .tight strong {
    color: var(--warn);
  }
</style>
