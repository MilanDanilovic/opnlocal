<script lang="ts">
  import { app } from "../lib/store.svelte";
  import { t } from "../lib/strings";
  import { focusOnMount } from "../lib/focus";
  import { api, backend } from "../lib/api";
  import Icon from "../components/Icon.svelte";
  import FitBadge from "../components/FitBadge.svelte";

  let { id }: { id: string } = $props();

  const model = $derived(app.model(id));
  const option = $derived(app.recs?.all.find((o) => o.model_id === id) ?? null);
  const accepted = $derived(model ? model.license.id in (app.settings?.accepted_licenses ?? {}) : false);
  const mobile = $derived(app.state?.platform === "android" || app.state?.platform === "ios");
  let agree = $state(false);
  let busy = $state(false);

  async function start() {
    if (!model) return;
    busy = true;
    if (model.license.requires_acceptance && !accepted) {
      app.state!.settings = await api.acceptLicense(model.license.id);
    }
    app.go({ name: "download", id: model.id });
    // The download screen shows progress; the call resolves when it's done.
    app.startDownload(model.id);
    busy = false;
  }

  async function openLicense() {
    if (model) (await backend()).openUrl(model.license.url);
  }
</script>

<main class="page">
  <div class="top">
    <button class="icon-btn" onclick={() => app.back()} aria-label={t.model.back}><Icon name="back" /></button>
  </div>
  {#if model}
    <div class="inner stack">
      <p class="muted small">{model.publisher}</p>
      <h1 use:focusOnMount>{model.name}</h1>
      <p class="lead">{model.summary}</p>

      <div class="card stack">
        {#if option}<FitBadge fit={option.fit} />{/if}
        <p class="row facts">
          <span><Icon name="download" /> {t.model.download(model.file.size)}</span>
          {#if option}<span><Icon name="disk" /> {t.model.needsSpace(option.storage.need_bytes)}</span>{/if}
        </p>
        {#if option && !option.storage.ok}
          <div class="banner bad" role="alert">{t.model.notEnoughSpace(option.storage.short_by_bytes)}</div>
        {/if}
        <p class="small">
          {t.model.license}: {model.license.name} ·
          <button class="link" onclick={openLicense}>{t.model.licenseLink} <Icon name="external" size={14} /></button>
        </p>
      </div>

      {#if model.license.requires_acceptance && !accepted}
        <section class="card stack terms" aria-labelledby="terms">
          <h2 id="terms">{t.model.acceptTitle}</h2>
          <p>{model.license.summary}</p>
          <label class="switch">
            <input type="checkbox" bind:checked={agree} />
            <span>{t.model.acceptCheckbox}</span>
          </label>
        </section>
      {/if}

      {#if mobile}<p class="muted small">{t.model.wifiHint}</p>{/if}

      <div class="row">
        <button
          class="btn big"
          disabled={busy || !app.online || (option !== null && !option.storage.ok) || (model.license.requires_acceptance && !accepted && !agree)}
          onclick={start}
        >
          <Icon name="download" /> {t.model.downloadNow(model.file.size - app.partialBytes(model.id))}
        </button>
      </div>
      {#if !app.online}<p class="muted small">{t.download.offline}</p>{/if}
    </div>
  {/if}
</main>

<style>
  .lead {
    font-size: 1.08rem;
    color: var(--text-2);
  }
  .facts {
    gap: 1.25rem;
    color: var(--text-2);
  }
  .facts span {
    display: inline-flex;
    gap: 0.4rem;
    align-items: center;
  }
  .terms {
    border-color: color-mix(in srgb, var(--warn) 45%, var(--border));
  }
  .link {
    background: none;
    border: none;
    padding: 0;
    color: var(--accent);
    font: inherit;
    text-decoration: underline;
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    gap: 0.25rem;
  }
</style>
