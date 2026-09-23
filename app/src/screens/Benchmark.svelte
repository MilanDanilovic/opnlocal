<script lang="ts">
  import { onMount } from "svelte";
  import { app } from "../lib/store.svelte";
  import { t } from "../lib/strings";
  import { focusOnMount } from "../lib/focus";
  import { asEngineError } from "../lib/api";
  import { seconds, number } from "../lib/format";
  import Icon from "../components/Icon.svelte";
  import type { BenchmarkResult } from "../lib/bindings/BenchmarkResult";

  let { id }: { id: string } = $props();
  let result = $state<BenchmarkResult | null>(null);
  let failed = $state<string | null>(null);

  async function run() {
    failed = null;
    result = null;
    try {
      result = await app.runBenchmark(id);
    } catch (e) {
      const err = asEngineError(e);
      if (err.kind === "llm") failed = "message" in err.error ? err.error.message : err.error.kind;
      else failed = "message" in err ? err.message : err.kind;
    }
  }

  onMount(run);
</script>

<main class="page">
  <div class="inner stack">
    {#if result}
      <h1 use:focusOnMount>{t.bench.measuredTitle}</h1>
      <section class="card result" aria-live="polite">
        <p class="eyebrow"><Icon name="bolt" /> {app.modelName(id)}</p>
        {#if result.words_per_second}
          <p class="big">{t.bench.speed(result.words_per_second)}</p>
          {#if result.verdict}<p class="verdict">{t.bench.verdict[result.verdict]}</p>{/if}
        {/if}
        <ul class="facts muted">
          <li>{t.bench.firstWord(seconds(result.first_token_ms))}</li>
          <li>{t.bench.loadTime(seconds(result.load_ms))}</li>
          <li>{t.bench.ranOn(result.device)}</li>
        </ul>
        <details>
          <summary>{t.bench.details}</summary>
          <table>
            <tbody>
              <tr><th>Prompt</th><td>{number(result.prompt_tokens)} tokens{#if result.prompt_tokens_per_second} · {Math.round(result.prompt_tokens_per_second)} tokens/s{/if}</td></tr>
              <tr><th>Reply</th><td>{number(result.generated_tokens)} tokens, {result.words} words{#if result.generation_tokens_per_second} · {result.generation_tokens_per_second.toFixed(1)} tokens/s{/if}</td></tr>
              <tr><th>Graphics layers</th><td>{result.gpu_layers}</td></tr>
              <tr><th>Context</th><td>{number(result.context)} tokens</td></tr>
              {#if result.ram_in_use_bytes}<tr><th>App memory</th><td>{(result.ram_in_use_bytes / 1e9).toFixed(1)} GB</td></tr>{/if}
              {#if result.gpu_memory_used_bytes}<tr><th>Graphics memory</th><td>{(result.gpu_memory_used_bytes / 1e9).toFixed(1)} GB</td></tr>{/if}
            </tbody>
          </table>
        </details>
      </section>
      <button class="btn big" onclick={() => app.finishSetup(id)}>{t.bench.startChat}</button>
    {:else if failed}
      <h1 use:focusOnMount>{t.bench.failedTitle}</h1>
      <div class="banner bad" role="alert"><Icon name="info" /><p>{failed}</p></div>
      <div class="row">
        <button class="btn" onclick={run}><Icon name="refresh" /> {t.bench.tryAgain}</button>
        <button class="btn secondary" onclick={() => app.finishSetup(id)}>{t.bench.chatAnyway}</button>
      </div>
    {:else}
      <h1 use:focusOnMount>{t.bench.title}</h1>
      <p class="muted">{t.bench.lead}</p>
      <div class="card stack" aria-live="polite">
        <progress aria-label={t.bench.title}></progress>
        <p>{app.benchStage ? t.bench.stage[app.benchStage] : t.bench.stage.loading}</p>
      </div>
      <button class="btn secondary" onclick={() => app.finishSetup(id)}>{t.bench.skip}</button>
    {/if}
  </div>
</main>

<style>
  .result {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }
  .eyebrow {
    display: flex;
    gap: 0.4rem;
    align-items: center;
    margin: 0;
    color: var(--accent);
    font-weight: 600;
  }
  .big {
    font-size: clamp(1.5rem, 1.1rem + 2vw, 2.2rem);
    font-weight: 700;
    line-height: 1.2;
    margin: 0.25rem 0 0;
  }
  .verdict {
    font-size: 1.1rem;
  }
  .facts {
    margin: 0;
    padding-left: 1.2rem;
  }
  summary {
    cursor: pointer;
    min-height: var(--tap);
    display: flex;
    align-items: center;
    font-weight: 600;
  }
  table {
    border-collapse: collapse;
    font-size: 0.9rem;
  }
  th {
    text-align: left;
    font-weight: 500;
    padding: 0.3rem 1rem 0.3rem 0;
    color: var(--text-2);
  }
</style>
