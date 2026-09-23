<script lang="ts">
  import { app } from "../lib/store.svelte";
  import { t } from "../lib/strings";
  import { focusOnMount } from "../lib/focus";
  import { api } from "../lib/api";
  import { bytes, wordsPerSecond } from "../lib/format";
  import Icon from "../components/Icon.svelte";

  let confirming = $state<string | null>(null);

  const models = $derived(
    (app.state?.installed ?? []).map((id) => {
      const catalog = app.model(id);
      const imported = app.state?.imported.find((m) => m.id === id);
      return { id, name: app.modelName(id), size: catalog?.file.size ?? imported?.size ?? 0, imported: !!imported };
    }),
  );

  async function remove(id: string) {
    await api.deleteModel(id);
    confirming = null;
    await app.refresh();
  }
</script>

<main class="page">
  <div class="top">
    <button class="btn ghost" onclick={() => app.back()}><Icon name="back" /> {t.settings.back}</button>
  </div>
  <div class="inner stack">
    <h1 use:focusOnMount>{t.models.title}</h1>
    <h2>{t.models.installed}</h2>
    {#if models.length === 0}
      <p class="muted">{t.models.none}</p>
    {/if}
    {#each models as m (m.id)}
      {@const b = app.benchmark(m.id)}
      <article class="card stack" aria-labelledby="im-{m.id}">
        <div class="row head">
          <h3 id="im-{m.id}">{m.name}</h3>
          <span class="muted small">{bytes(m.size)}</span>
          {#if app.settings?.active_model === m.id}<span class="pill">{t.models.active}</span>{/if}
        </div>
        {#if m.imported}<p class="small muted">{t.models.imported}</p>{/if}
        <p class="small">
          {#if b?.words_per_second}
            <strong>{t.bench.measuredTitle}:</strong> {t.bench.speed(b.words_per_second).toLowerCase()} · {t.bench.ranOn(b.device)}
          {:else}
            <span class="muted">{t.models.notMeasured}</span>
          {/if}
        </p>
        <div class="row">
          {#if app.settings?.active_model !== m.id}
            <button class="btn secondary" onclick={() => app.saveSettings({ active_model: m.id })}>{t.models.use}</button>
          {/if}
          <button class="btn secondary" onclick={() => app.go({ name: "benchmark", id: m.id })}><Icon name="bolt" /> {t.models.rerun}</button>
          {#if confirming === m.id}
            <span class="small">{t.models.deleteConfirm(m.name, m.size)}</span>
            <button class="btn danger" onclick={() => remove(m.id)}>{t.models.delete}</button>
            <button class="btn ghost" onclick={() => (confirming = null)}>{t.model.back}</button>
          {:else}
            <button class="btn danger" onclick={() => (confirming = m.id)}><Icon name="trash" /> {t.models.delete}</button>
          {/if}
        </div>
      </article>
    {/each}
    <button class="btn" onclick={() => app.go({ name: "use_case" })}><Icon name="plus" /> {t.models.add}</button>
  </div>
</main>

<style>
  .head {
    align-items: baseline;
  }
  h3 {
    margin: 0;
  }
  .pill {
    font-size: 0.8rem;
    font-weight: 600;
    padding: 0.15rem 0.6rem;
    border-radius: 999px;
    background: var(--accent-soft);
    color: var(--accent);
  }
</style>
