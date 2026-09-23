<script lang="ts">
  import { app } from "../lib/store.svelte";
  import { t } from "../lib/strings";
  import { focusOnMount } from "../lib/focus";
  import { percent } from "../lib/format";
  import Icon from "../components/Icon.svelte";
  import type { DownloadError } from "../lib/bindings/DownloadError";
  import type { EngineError } from "../lib/bindings/EngineError";

  let { id }: { id: string } = $props();
  let confirming = $state(false);

  const model = $derived(app.model(id));
  const dl = $derived(app.download?.modelId === id ? app.download : null);
  const p = $derived(dl?.progress ?? null);
  const pct = $derived(p && "total" in p ? percent(p.done, p.total) : 0);

  // Done: move on to the speed test (replacing this screen in history).
  $effect(() => {
    if (dl?.status === "done") {
      const next = { name: "benchmark" as const, id };
      history.replaceState({ screen: next }, "");
      app.screen = next;
    }
    if (dl?.status === "cancelled") app.back();
  });

  function message(e: DownloadError | EngineError): string {
    switch (e.kind) {
      case "offline":
        return t.download.offline;
      case "not_enough_space":
        return t.download.noSpace("need_bytes" in e ? e.need_bytes : 0);
      case "corrupt":
        return t.download.corrupt;
      case "http":
        return t.download.http(e.status);
      case "network":
        return t.download.network;
      case "io":
        return t.download.io(e.message);
      case "download_in_progress":
        return t.download.elsewhere;
      default:
        return t.errors.generic;
    }
  }

  function retry() {
    app.startDownload(id);
  }
</script>

<main class="page">
  <div class="top">
    <button class="icon-btn" onclick={() => app.back()} aria-label={t.model.back}><Icon name="back" /></button>
  </div>
  <div class="inner stack">
    <h1 use:focusOnMount>{t.download.title(model?.name ?? id)}</h1>

    {#if dl?.status === "failed" && dl.error}
      <div class="banner bad" role="alert">
        <Icon name={dl.error.kind === "offline" ? "wifiOff" : "info"} />
        <div>
          <strong>{t.download.failedTitle}</strong>
          <p>{message(dl.error)}</p>
          <button class="btn" onclick={retry} disabled={!app.online && dl.error.kind === "offline"}>
            <Icon name="refresh" /> {t.download.retry}
          </button>
        </div>
      </div>
    {:else}
      <div class="card stack">
        <progress max="100" value={pct} aria-label={t.download.title(model?.name ?? id)} aria-valuetext="{pct}%"></progress>
        <div class="row between" aria-live="polite">
          {#if p?.stage === "downloading"}
            <span>{t.download.progress(p.done, p.total)}</span>
            {#if p.bytes_per_second}
              <span class="muted">{t.download.remaining((p.total - p.done) / p.bytes_per_second)}</span>
            {/if}
          {:else if p?.stage === "checking_partial"}
            <span>{t.download.checking}</span>
          {:else if p?.stage === "retrying"}
            <span class="warn">{t.download.retrying(p.wait_seconds)}</span>
          {:else if dl?.status === "done"}
            <span>{t.download.done}</span>
          {:else}
            <span class="muted">…</span>
          {/if}
        </div>
        <p class="muted small">{t.download.keepOpen}</p>
      </div>

      {#if dl?.status === "running"}
        {#if confirming}
          <div class="banner warn" role="alertdialog" aria-labelledby="cancel-q">
            <p id="cancel-q">{t.download.cancelConfirm}</p>
            <div class="row">
              <button class="btn danger" onclick={() => app.cancelDownload()}>{t.download.cancel}</button>
              <button class="btn secondary" onclick={() => (confirming = false)}>{t.model.back}</button>
            </div>
          </div>
        {:else}
          <button class="btn secondary" onclick={() => (confirming = true)}>{t.download.cancel}</button>
        {/if}
      {/if}
    {/if}
  </div>
</main>

<style>
  .between {
    justify-content: space-between;
  }
  .warn {
    color: var(--warn);
    font-weight: 600;
  }
  .banner p {
    margin: 0.25rem 0 0.75rem;
  }
  .banner[role="alertdialog"] {
    flex-direction: column;
  }
</style>
