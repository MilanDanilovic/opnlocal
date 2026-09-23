<script lang="ts">
  import { app } from "../lib/store.svelte";
  import { t } from "../lib/strings";
  import { bytes } from "../lib/format";
  import FitBadge from "./FitBadge.svelte";
  import type { ModelOption } from "../lib/bindings/ModelOption";
  import type { Slot } from "../lib/bindings/Slot";

  let { option, slot, compact = false }: { option: ModelOption; slot?: Slot; compact?: boolean } = $props();

  const model = $derived(app.model(option.model_id));
  const partial = $derived(app.partialBytes(option.model_id));
  const tooBig = $derived(option.fit.label === "too_big");
  const unsuited = $derived(option.score === 0);
</script>

{#if model}
  <article class="card model" class:dim={tooBig} class:compact aria-labelledby="m-{model.id}">
    {#if slot}<p class="slot {slot}">{t.recommend.slot[slot]}</p>{/if}
    <div class="head">
      <h3 id="m-{model.id}">{model.name}</h3>
      <span class="muted small">{model.publisher}</span>
    </div>
    {#if !compact}<p class="summary">{model.summary}</p>{/if}
    <FitBadge fit={option.fit} />
    <p class="meta muted small">
      {t.model.download(model.file.size)} · {t.model.needsSpace(option.storage.need_bytes)} · {model.license.name}
      {#if unsuited} · {t.recommend.notForThis}{/if}
    </p>
    {#if !option.storage.ok && !option.installed}
      <p class="space small" role="note">{t.model.notEnoughSpace(option.storage.short_by_bytes)}</p>
    {/if}
    {#if !tooBig}
      <div class="actions">
        {#if option.installed}
          <button
            class="btn secondary"
            onclick={() => (app.benchmark(model.id) ? app.finishSetup(model.id) : app.go({ name: "benchmark", id: model.id }))}
          >
            {t.recommend.open}
          </button>
        {:else}
          <button
            class="btn"
            class:secondary={slot !== "recommended"}
            onclick={() => app.go({ name: "model", id: model.id })}
            aria-describedby="m-{model.id}"
          >
            {partial > 0 ? t.recommend.resume : t.recommend.download}
            {#if partial > 0}<span class="small">({bytes(partial)})</span>{/if}
          </button>
        {/if}
      </div>
    {/if}
  </article>
{/if}

<style>
  .model {
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
  }
  .model.compact {
    padding: 1rem;
  }
  .dim {
    opacity: 0.72;
    box-shadow: none;
  }
  .slot {
    margin: 0;
    font-size: 0.8rem;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--accent);
  }
  .slot.stronger {
    color: var(--warn);
  }
  .head {
    display: flex;
    align-items: baseline;
    gap: 0.6rem;
    flex-wrap: wrap;
  }
  h3 {
    margin: 0;
    font-size: 1.2rem;
  }
  .summary {
    margin: 0;
  }
  .meta {
    margin: 0;
  }
  .space {
    margin: 0;
    color: var(--bad);
    font-weight: 600;
  }
  .actions {
    margin-top: 0.25rem;
  }
</style>
