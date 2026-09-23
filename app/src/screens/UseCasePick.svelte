<script lang="ts">
  import { app } from "../lib/store.svelte";
  import { t } from "../lib/strings";
  import { focusOnMount } from "../lib/focus";
  import Icon from "../components/Icon.svelte";
  import type { UseCase } from "../lib/bindings/UseCase";

  const options: { id: UseCase; icon: string }[] = [
    { id: "everyday", icon: "chat" },
    { id: "coding", icon: "code" },
    { id: "writing", icon: "pen" },
    { id: "documents", icon: "doc" },
  ];
  let busy = $state<UseCase | null>(null);

  async function pick(id: UseCase) {
    busy = id;
    try {
      await app.chooseUseCase(id);
    } finally {
      busy = null;
    }
  }
</script>

<main class="page">
  <div class="top">
    <button class="icon-btn" onclick={() => app.back()} aria-label={t.model.back}><Icon name="back" /></button>
  </div>
  <div class="inner">
    <h1 use:focusOnMount>{t.useCase.title}</h1>
    <p class="muted">{t.useCase.lead}</p>
    <div class="grid">
      {#each options as o (o.id)}
        <button
          class="option"
          class:selected={app.settings?.use_case === o.id}
          aria-busy={busy === o.id}
          disabled={busy !== null}
          onclick={() => pick(o.id)}
        >
          <span class="icon"><Icon name={o.icon} size={26} /></span>
          <span class="title">{t.useCase.options[o.id].title}</span>
          <span class="body">{t.useCase.options[o.id].body}</span>
        </button>
      {/each}
    </div>
  </div>
</main>

<style>
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
    gap: 0.9rem;
    margin-top: 1.25rem;
  }
  .option {
    display: grid;
    grid-template-columns: auto 1fr;
    grid-template-rows: auto auto;
    column-gap: 0.9rem;
    text-align: left;
    padding: 1.1rem;
    border-radius: var(--radius-l);
    border: 1px solid var(--border);
    background: var(--surface);
    color: var(--text);
    font: inherit;
    cursor: pointer;
    box-shadow: var(--shadow);
    transition: border-color 0.15s, transform 0.05s;
  }
  .option:hover {
    border-color: var(--accent);
  }
  .option.selected {
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .option:disabled {
    cursor: progress;
  }
  .icon {
    grid-row: span 2;
    display: grid;
    place-items: center;
    width: 48px;
    height: 48px;
    border-radius: 14px;
    background: var(--accent-soft);
    color: var(--accent);
  }
  .title {
    font-weight: 650;
    font-size: 1.05rem;
    align-self: end;
  }
  .body {
    color: var(--text-2);
    font-size: 0.93rem;
  }
</style>
