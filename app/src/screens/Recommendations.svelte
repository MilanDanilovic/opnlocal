<script lang="ts">
  import { onMount } from "svelte";
  import { app } from "../lib/store.svelte";
  import { t } from "../lib/strings";
  import { focusOnMount } from "../lib/focus";
  import Icon from "../components/Icon.svelte";
  import ModelCard from "../components/ModelCard.svelte";

  let showAll = $state(false);

  onMount(() => {
    if (!app.recs) app.loadRecommendations();
  });

  const recs = $derived(app.recs);
  const pickedIds = $derived(new Set(recs?.picks.map((p) => p.option.model_id)));
  const others = $derived(recs?.all.filter((o) => !pickedIds.has(o.model_id)) ?? []);
</script>

<main class="page">
  <div class="top">
    <button class="icon-btn" onclick={() => app.back()} aria-label={t.model.back}><Icon name="back" /></button>
  </div>
  <div class="inner">
    {#if recs}
      {#if recs.unsupported}
        <h1 use:focusOnMount>
          {recs.unsupported.kind === "cpu_too_old" ? t.recommend.cpuTooOldTitle : t.recommend.notEnoughMemoryTitle}
        </h1>
        <div class="banner bad" role="alert">
          <Icon name="info" />
          <div>
            {#if recs.unsupported.kind === "cpu_too_old"}
              <p>{t.device.cpuTooOld}</p>
            {:else}
              <p>{t.recommend.notEnoughMemory(recs.unsupported.smallest_need_bytes)}</p>
            {/if}
            <p class="small">{t.recommend.importInstead}</p>
          </div>
        </div>
      {:else}
        <h1 use:focusOnMount>{t.recommend.title(recs.use_case)}</h1>
        <p class="muted small note"><Icon name="info" size={16} /> {t.recommend.estimateNote}</p>
        <div class="picks">
          {#each recs.picks as p (p.option.model_id)}
            <ModelCard option={p.option} slot={p.slot} />
          {/each}
        </div>
      {/if}

      {#if others.length}
        <button class="btn ghost" aria-expanded={showAll} onclick={() => (showAll = !showAll)}>
          <Icon name="down" />
          {showAll ? t.recommend.hideAll : t.recommend.seeAll}
        </button>
        {#if showAll}
          <div class="all">
            {#each others as o (o.model_id)}
              <ModelCard option={o} compact />
            {/each}
          </div>
        {/if}
      {/if}

      <p><button class="link" onclick={() => app.go({ name: "use_case" })}>{t.recommend.changeUse}</button></p>
    {:else}
      <h1 use:focusOnMount>{t.device.checking}</h1>
    {/if}
  </div>
</main>

<style>
  .note {
    display: flex;
    gap: 0.4rem;
    align-items: flex-start;
  }
  .note :global(svg) {
    flex: none;
    margin-top: 0.15em;
  }
  .picks,
  .all {
    display: grid;
    gap: 1rem;
    margin: 1.25rem 0;
  }
  .all {
    grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
  }
  .link {
    background: none;
    border: none;
    padding: 0.5rem 0;
    min-height: var(--tap);
    color: var(--accent);
    font: inherit;
    font-weight: 600;
    text-decoration: underline;
    cursor: pointer;
  }
</style>
