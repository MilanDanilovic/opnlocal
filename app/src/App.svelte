<script lang="ts">
  import { onMount } from "svelte";
  import { app } from "./lib/store.svelte";
  import { t } from "./lib/strings";
  import { percent } from "./lib/format";
  import Icon from "./components/Icon.svelte";
  import Welcome from "./screens/Welcome.svelte";
  import DeviceCheck from "./screens/DeviceCheck.svelte";
  import UseCasePick from "./screens/UseCasePick.svelte";
  import Recommendations from "./screens/Recommendations.svelte";
  import ModelDetails from "./screens/ModelDetails.svelte";
  import Download from "./screens/Download.svelte";
  import Benchmark from "./screens/Benchmark.svelte";
  import Chat from "./screens/Chat.svelte";
  import Models from "./screens/Models.svelte";
  import SettingsScreen from "./screens/Settings.svelte";

  let ready = $state(false);
  let failed = $state<string | null>(null);

  onMount(() => {
    app.init().then(
      () => (ready = true),
      (e) => (failed = String(e?.message ?? e)),
    );
  });

  const screen = $derived(app.screen);
  // Show download progress everywhere except on the download screen itself.
  const dl = $derived(app.download);
  const showDownloadBanner = $derived(dl?.status === "running" && screen.name !== "download");
  const dlPct = $derived(dl?.progress && "total" in dl.progress ? percent(dl.progress.done, dl.progress.total) : 0);
</script>

{#if failed}
  <main class="page"><div class="inner card" role="alert"><h1>{t.errors.generic}</h1><p class="muted">{failed}</p></div></main>
{:else if !ready}
  <div class="boot" aria-busy="true" aria-label="Loading"></div>
{:else}
  <div class="shell">
    {#if !app.online}
      <div class="global-banner warn" role="status"><Icon name="wifiOff" /> {t.offline}</div>
    {/if}
    {#if showDownloadBanner && dl}
      <button class="global-banner" onclick={() => app.go({ name: "download", id: dl.modelId })}>
        <Icon name="download" />
        <span>{t.download.banner(app.modelName(dl.modelId) + (dl.vision ? t.download.visionSuffix : ""), dlPct)}</span>
        <progress max="100" value={dlPct} aria-hidden="true"></progress>
      </button>
    {/if}
    <div class="screen">
      {#if screen.name === "welcome"}
        <Welcome />
      {:else if screen.name === "device"}
        <DeviceCheck />
      {:else if screen.name === "use_case"}
        <UseCasePick />
      {:else if screen.name === "recommend"}
        <Recommendations />
      {:else if screen.name === "model"}
        {#key screen.id}<ModelDetails id={screen.id} />{/key}
      {:else if screen.name === "download"}
        {#key screen.id}<Download id={screen.id} />{/key}
      {:else if screen.name === "benchmark"}
        {#key screen.id}<Benchmark id={screen.id} />{/key}
      {:else if screen.name === "chat"}
        <Chat />
      {:else if screen.name === "models"}
        <Models />
      {:else if screen.name === "settings"}
        <SettingsScreen />
      {/if}
    </div>
  </div>
{/if}

<style>
  .boot {
    height: 100%;
  }
  .shell {
    height: 100%;
    display: flex;
    flex-direction: column;
  }
  .screen {
    flex: 1;
    min-height: 0;
    overflow: auto;
  }
  .global-banner {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    padding: calc(0.5rem + var(--safe-top)) 1rem 0.5rem;
    background: var(--accent-soft);
    color: var(--text);
    border: none;
    border-bottom: 1px solid var(--border);
    font: inherit;
    font-size: 0.9rem;
    text-align: left;
    width: 100%;
    cursor: pointer;
  }
  .global-banner.warn {
    background: var(--warn-soft);
    cursor: default;
  }
  .global-banner progress {
    width: 120px;
    margin-left: auto;
  }
</style>
