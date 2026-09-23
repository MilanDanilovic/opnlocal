<script lang="ts">
  import { onMount } from "svelte";
  import { app } from "../lib/store.svelte";
  import { t } from "../lib/strings";
  import { focusOnMount } from "../lib/focus";
  import { api, asEngineError, backend } from "../lib/api";
  import { bytes } from "../lib/format";
  import Icon from "../components/Icon.svelte";
  import notices from "../lib/notices.json";
  import type { ModelSettings } from "../lib/bindings/ModelSettings";

  let catalogStatus = $state<string | null>(null);
  let checking = $state(false);
  let confirmWipe = $state(false);
  let importMsg = $state<string | null>(null);
  let selectedModel = $state<string>(app.settings?.active_model ?? app.state?.installed[0] ?? "");
  let savedFlash = $state(false);

  onMount(() => {
    if (!app.device) app.detect();
  });

  const s = $derived(app.settings);
  const ms = $derived<ModelSettings>(
    (s?.per_model[selectedModel] as ModelSettings | undefined) ?? {
      context: null, temperature: null, top_p: null, max_reply_tokens: null, system_prompt: null, gpu_layers: null, threads: null, use_integrated_gpu: false,
    },
  );

  async function checkNow() {
    checking = true;
    catalogStatus = null;
    try {
      const r = await api.refreshCatalog(true);
      catalogStatus = r.updated ? t.settings.updated(r.version) : t.settings.checked(r.version);
      await app.refresh();
    } catch {
      catalogStatus = t.settings.checkFailed;
    } finally {
      checking = false;
    }
  }

  async function saveModel(patch: Partial<ModelSettings>) {
    const per = { ...$state.snapshot(s!.per_model) };
    per[selectedModel] = { ...ms, ...patch };
    await app.saveSettings({ per_model: per });
    await api.unload(); // the next reply reloads the model with the new settings
    savedFlash = true;
    setTimeout(() => (savedFlash = false), 1200);
  }

  function num(v: string): number | null {
    const n = Number(v);
    return v.trim() === "" || !isFinite(n) ? null : n;
  }

  async function importModel() {
    importMsg = null;
    const path = await (await backend()).pickFile({ documents: false });
    if (!path) return;
    try {
      const m = await api.importModel(path);
      importMsg = `${m.name} · ${bytes(m.size)}`;
      await app.refresh();
    } catch (e) {
      const err = asEngineError(e);
      importMsg = "message" in err ? String(err.message) : t.errors.generic;
    } finally {
      app.importFraction = null;
    }
  }

  async function wipe() {
    await api.deleteAllConversations();
    app.conversation = null;
    confirmWipe = false;
    await app.refresh();
  }
</script>

<main class="page">
  <div class="top">
    <button class="btn ghost" onclick={() => app.back()}><Icon name="back" /> {t.settings.back}</button>
  </div>
  <div class="inner stack">
    <h1 use:focusOnMount>{t.settings.title}</h1>

    <section class="card stack" aria-labelledby="privacy">
      <h2 id="privacy"><Icon name="lock" /> {t.settings.privacy}</h2>
      <p>{t.settings.privacyLead}</p>
      <dl>
        {#each t.settings.connections as [what, detail] (what)}
          <dt>{what}</dt>
          <dd class="muted">{detail}</dd>
        {/each}
      </dl>
      <p class="small">{t.settings.never}</p>
      <label class="switch">
        <input type="checkbox" checked={s?.auto_refresh_catalog} onchange={(e) => app.saveSettings({ auto_refresh_catalog: e.currentTarget.checked })} />
        <span>{t.settings.autoRefresh}</span>
      </label>
      <div class="row">
        <button class="btn secondary" onclick={checkNow} disabled={checking || !app.online}><Icon name="refresh" /> {t.settings.checkNow}</button>
        {#if catalogStatus}<span class="small" role="status">{catalogStatus}</span>{/if}
      </div>
      <div class="row">
        {#if confirmWipe}
          <span class="small">{t.settings.deleteAllConfirm}</span>
          <button class="btn danger" onclick={wipe}>{t.models.delete}</button>
          <button class="btn ghost" onclick={() => (confirmWipe = false)}>{t.model.back}</button>
        {:else}
          <button class="btn danger" onclick={() => (confirmWipe = true)}><Icon name="trash" /> {t.settings.deleteAll}</button>
        {/if}
      </div>
    </section>

    <details class="card advanced">
      <summary><h2><Icon name="settings" /> {t.settings.advanced}</h2></summary>
      <div class="stack">
        <p class="muted small">{t.settings.advancedLead}</p>

        {#if s?.gpu_crash}
          <div class="banner warn" role="status">{t.settings.gpuCrash(s.gpu_crash.device)}</div>
        {/if}
        <label class="switch">
          <input
            type="checkbox"
            checked={!s?.gpu_disabled && !s?.gpu_crash}
            onchange={async (e) => { await app.saveSettings({ gpu_disabled: !e.currentTarget.checked, gpu_crash: null }); await api.unload(); }}
          />
          <span>{t.settings.gpu}</span>
        </label>

        {#if (app.state?.installed.length ?? 0) > 0}
          <h3>{t.settings.perModel}</h3>
          <select bind:value={selectedModel} aria-label={t.chat.model}>
            {#each app.state?.installed ?? [] as id (id)}
              <option value={id}>{app.modelName(id)}</option>
            {/each}
          </select>
          {#key selectedModel}
            <div class="grid">
              <label>{t.settings.context}<input type="number" min="2048" step="1024" value={ms.context ?? ""} placeholder="auto" onchange={(e) => saveModel({ context: num(e.currentTarget.value) })} /><span class="muted small">{t.settings.contextHint}</span></label>
              <label>{t.settings.temperature}<input type="number" min="0" max="2" step="0.1" value={ms.temperature ?? ""} placeholder="auto" onchange={(e) => saveModel({ temperature: num(e.currentTarget.value) })} /></label>
              <label>{t.settings.topP}<input type="number" min="0" max="1" step="0.05" value={ms.top_p ?? ""} placeholder="auto" onchange={(e) => saveModel({ top_p: num(e.currentTarget.value) })} /></label>
              <label>{t.settings.maxReply}<input type="number" min="64" step="64" value={ms.max_reply_tokens ?? ""} placeholder="auto" onchange={(e) => saveModel({ max_reply_tokens: num(e.currentTarget.value) })} /></label>
              <label>{t.settings.gpuLayers}<input type="number" min="0" value={ms.gpu_layers ?? ""} placeholder="auto" onchange={(e) => saveModel({ gpu_layers: num(e.currentTarget.value) })} /></label>
              <label>{t.settings.threads}<input type="number" min="1" value={ms.threads ?? ""} placeholder="auto" onchange={(e) => saveModel({ threads: num(e.currentTarget.value) })} /></label>
            </div>
            <label class="full">{t.settings.systemPrompt}<textarea rows="3" value={ms.system_prompt ?? ""} placeholder="auto" onchange={(e) => saveModel({ system_prompt: e.currentTarget.value.trim() || null })}></textarea></label>
            <div class="row">
              <button class="btn secondary" onclick={() => saveModel({ context: null, temperature: null, top_p: null, max_reply_tokens: null, system_prompt: null, gpu_layers: null, threads: null })}>{t.settings.reset}</button>
              {#if savedFlash}<span class="small" role="status">{t.settings.saved}</span>{/if}
            </div>
          {/key}
        {/if}

        <h3>{t.settings.hardware}</h3>
        {#if app.device}
          <!-- Scrollable, so it must be keyboard-focusable (WCAG 2.1.1). -->
          <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
          <pre class="report" tabindex="0" aria-label={t.settings.hardware}>{JSON.stringify(app.device, null, 2)}</pre>
        {/if}

        {#if Object.keys(app.state?.benchmarks ?? {}).length}
          <h3>{t.settings.benchmarks}</h3>
          <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
          <pre class="report" tabindex="0" aria-label={t.settings.benchmarks}>{JSON.stringify(app.state?.benchmarks, null, 2)}</pre>
        {/if}

        <h3>{t.settings.storage}</h3>
        <p class="small">{t.settings.storagePath}<br /><code>{app.state?.storage_path}</code></p>
        {#if app.device}<p class="small muted">{t.settings.storageFree(app.device.storage.free)}</p>{/if}

        <button class="btn secondary" onclick={importModel}><Icon name="download" /> {t.settings.importModel}</button>
        <p class="small muted">{t.settings.importHint}</p>
        {#if app.importFraction !== null}<progress max="1" value={app.importFraction} aria-label={t.settings.importModel}></progress>{/if}
        {#if importMsg}<p class="small" role="status">{importMsg}</p>{/if}
      </div>
    </details>

    <section class="card stack" aria-labelledby="about">
      <h2 id="about"><Icon name="info" /> {t.settings.about}</h2>
      <p>{t.settings.version(app.state?.app_version ?? "")}</p>
      <details>
        <summary>{t.settings.notices}</summary>
        <p class="small muted">{t.settings.noticesLead}</p>
        <ul class="notices small">
          {#each notices as n (n.name)}
            <li><strong>{n.name}</strong> · {n.license}{#if n.note} · <span class="muted">{n.note}</span>{/if}</li>
          {/each}
        </ul>
      </details>
    </section>
  </div>
</main>

<style>
  h2 {
    display: flex;
    gap: 0.5rem;
    align-items: center;
    margin: 0;
  }
  h3 {
    margin-top: 1rem;
  }
  dt {
    font-weight: 600;
  }
  dd {
    margin: 0.1rem 0 0.6rem;
  }
  .advanced summary {
    cursor: pointer;
    list-style: none;
    min-height: var(--tap);
    display: flex;
    align-items: center;
  }
  .advanced[open] summary {
    margin-bottom: 0.75rem;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
    gap: 0.9rem;
  }
  .grid label,
  .full {
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
    font-size: 0.92rem;
  }
  .report {
    max-height: 260px;
    overflow: auto;
    background: var(--surface-2);
    border-radius: var(--radius-s);
    padding: 0.75rem;
    font: 0.8rem/1.45 var(--mono);
  }
  code {
    font-family: var(--mono);
    font-size: 0.85rem;
    overflow-wrap: anywhere;
  }
  .notices {
    padding-left: 1.2rem;
  }
  summary {
    cursor: pointer;
    min-height: var(--tap);
    display: flex;
    align-items: center;
    font-weight: 600;
  }
</style>
