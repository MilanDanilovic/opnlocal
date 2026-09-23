<script lang="ts">
  import { onMount } from "svelte";
  import { app } from "../lib/store.svelte";
  import { t } from "../lib/strings";
  import { focusOnMount } from "../lib/focus";
  import { asEngineError } from "../lib/api";
  import Icon from "../components/Icon.svelte";

  let error = $state<string | null>(null);

  async function run() {
    error = null;
    try {
      await app.detect();
    } catch (e) {
      error = asEngineError(e).kind;
    }
  }

  onMount(run);

  const d = $derived(app.device);
  const tooOld = $derived(d?.issues.find((i) => i.kind === "cpu_too_old"));
  const gpu = $derived(d?.gpus.find((g) => g.kind !== "integrated") ?? d?.gpus[0]);
</script>

<main class="page">
  <div class="top">
    <button class="icon-btn" onclick={() => app.back()} aria-label={t.model.back}><Icon name="back" /></button>
  </div>
  <div class="inner">
    <h1 use:focusOnMount>{d ? t.device.heading : t.device.title}</h1>

    <div aria-live="polite">
      {#if !d && !error}
        <div class="checking">
          <span class="spinner" aria-hidden="true"></span>
          <p class="muted">{t.device.checking}</p>
        </div>
      {:else if error}
        <div class="banner bad" role="alert">{t.errors.generic}</div>
      {:else if d}
        <ul class="facts card">
          {#if d.device_name}
            <li><Icon name="chip" /><span><strong>{d.device_name}</strong> · {d.os_version}</span></li>
          {/if}
          <li><Icon name="memory" /><span>{t.device.memory(d.memory.total)}</span></li>
          <li>
            <Icon name="gpu" />
            <span>
              {#if gpu}
                {gpu.kind === "discrete" ? t.device.graphicsName(gpu.name, gpu.memory_total) : t.device.graphicsShared(gpu.name)}
              {:else}
                <span class="muted">{t.device.graphicsNone}</span>
              {/if}
            </span>
          </li>
          <li><Icon name="chip" /><span>{d.cpu.name}</span></li>
          <li><Icon name="disk" /><span>{t.device.storage(d.storage.free)}</span></li>
        </ul>

        {#if tooOld}
          <div class="banner bad" role="alert">
            <Icon name="info" />
            <div>
              <strong>{t.recommend.cpuTooOldTitle}</strong>
              <p>{t.device.cpuTooOld}</p>
            </div>
          </div>
        {/if}
      {/if}
    </div>

    <div class="row actions">
      {#if d && !tooOld}
        <button class="btn big" onclick={() => app.go({ name: "use_case" })}>{t.device.continue}</button>
      {/if}
      {#if d || error}
        <button class="btn secondary" onclick={run}>{t.device.again}</button>
      {/if}
    </div>
  </div>
</main>

<style>
  .checking {
    display: flex;
    align-items: center;
    gap: 1rem;
    padding: 2rem 0;
  }
  .spinner {
    width: 28px;
    height: 28px;
    border-radius: 50%;
    border: 3px solid var(--surface-3);
    border-top-color: var(--accent);
    animation: spin 0.9s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  .facts {
    list-style: none;
    margin: 1rem 0;
    padding: 0.5rem 1.25rem;
  }
  .facts li {
    display: flex;
    gap: 0.9rem;
    align-items: center;
    padding: 0.75rem 0;
    color: var(--text);
  }
  .facts li + li {
    border-top: 1px solid var(--border);
  }
  .facts li :global(svg) {
    flex: none;
    color: var(--accent);
  }
  .actions {
    margin-top: 1.5rem;
  }
</style>
