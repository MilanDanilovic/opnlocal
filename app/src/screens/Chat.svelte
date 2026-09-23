<script lang="ts">
  import { tick } from "svelte";
  import { app } from "../lib/store.svelte";
  import { t } from "../lib/strings";
  import { day } from "../lib/format";
  import Icon from "../components/Icon.svelte";
  import Logo from "../components/Logo.svelte";
  import MessageView from "../components/MessageView.svelte";
  import Composer from "../components/Composer.svelte";
  import type { UseCase } from "../lib/bindings/UseCase";

  let drawerOpen = $state(false);
  let confirmDelete = $state(false);
  let list = $state<HTMLDivElement>();
  let composer = $state<Composer>();
  let pinnedToBottom = true;
  let dropped = $state(0);

  const conv = $derived(app.conversation);
  const modelId = $derived(conv?.model_id ?? app.settings?.active_model ?? null);
  const installed = $derived(app.state?.installed ?? []);
  const hasModel = $derived(modelId !== null && installed.includes(modelId));
  const catalogModel = $derived(app.model(modelId));
  const canThink = $derived(!catalogModel || catalogModel.chat.thinking.kind !== "unsupported");
  const useCase = $derived<UseCase>(conv?.use_case ?? app.settings?.use_case ?? "everyday");
  const stream = $derived(app.streaming);
  const lastAssistant = $derived(conv?.messages.findLastIndex((m) => m.role === "assistant") ?? -1);

  $effect(() => {
    if (stream && stream.droppedMessages) dropped = stream.droppedMessages;
  });

  // Keep the newest text in view while it streams, unless the user scrolled up to read.
  $effect(() => {
    void stream?.answer;
    void stream?.thinking;
    void conv?.messages.length;
    if (pinnedToBottom && list) tick().then(() => list?.scrollTo({ top: list.scrollHeight }));
  });

  function onScroll() {
    if (list) pinnedToBottom = list.scrollHeight - list.scrollTop - list.clientHeight < 80;
  }

  // Copy buttons inside rendered code blocks (plain HTML from Markdown, so one delegated listener).
  $effect(() => {
    const el = list;
    el?.addEventListener("click", onClick);
    return () => el?.removeEventListener("click", onClick);
  });

  function onClick(e: MouseEvent) {
    const btn = (e.target as HTMLElement).closest("[data-copy]");
    if (!btn) return;
    const code = btn.closest(".code")?.querySelector("code")?.textContent ?? "";
    navigator.clipboard.writeText(code);
    btn.textContent = t.chat.copied;
    setTimeout(() => (btn.textContent = t.chat.copy), 1500);
  }

  async function open(id: string) {
    drawerOpen = false;
    dropped = 0;
    await app.openConversation(id);
    app.go({ name: "chat", conversationId: id });
  }

  function newChat() {
    drawerOpen = false;
    dropped = 0;
    app.newChat();
  }

  async function send(prompt: string) {
    dropped = 0;
    await app.send(prompt, [], false);
  }

  const installedModels = $derived(
    installed.map((id) => ({ id, name: app.modelName(id) })).sort((a, b) => a.name.localeCompare(b.name)),
  );

  function errorText(): string {
    const e = app.chatError;
    if (!e) return "";
    if (e.kind === "message_too_long") return t.chat.tooLong(e.times_too_long);
    if (e.kind === "llm" && e.error.kind === "context_full") return t.chat.tooLong(1);
    return t.chat.error;
  }
</script>

<div class="layout" class:drawer-open={drawerOpen}>
  <aside class="sidebar" aria-label={t.chat.conversations}>
    <div class="side-top">
      <Logo size={30} />
      <button class="icon-btn close" onclick={() => (drawerOpen = false)} aria-label={t.errors.dismiss}><Icon name="x" /></button>
    </div>
    <button class="btn new" onclick={newChat}><Icon name="plus" /> {t.chat.newChat}</button>
    <nav class="convs">
      <h2 class="visually-hidden">{t.chat.conversations}</h2>
      <ul>
        {#each app.state?.conversations ?? [] as c (c.id)}
          <li>
            <button class="conv" class:active={conv?.id === c.id} aria-current={conv?.id === c.id ? "page" : undefined} onclick={() => open(c.id)}>
              <span class="title">{c.title}</span>
              <span class="muted small">{day(c.updated_at)}</span>
            </button>
          </li>
        {/each}
      </ul>
    </nav>
    <div class="side-bottom">
      <button class="btn ghost" onclick={() => app.go({ name: "models" })}><Icon name="chip" /> {t.models.title}</button>
      <button class="btn ghost" onclick={() => app.go({ name: "settings" })}><Icon name="settings" /> {t.chat.settings}</button>
    </div>
  </aside>
  <button class="scrim" aria-hidden="true" tabindex="-1" onclick={() => (drawerOpen = false)}></button>

  <main class="main">
    <header class="bar">
      <button class="icon-btn menu" onclick={() => (drawerOpen = true)} aria-label={t.chat.menu}><Icon name="menu" /></button>
      {#if hasModel}
        <label class="visually-hidden" for="model-pick">{t.chat.model}</label>
        <select id="model-pick" class="model-pick" value={modelId} disabled={app.generating} onchange={(e) => app.switchModel(e.currentTarget.value)}>
          {#each installedModels as m (m.id)}
            <option value={m.id}>{m.name}</option>
          {/each}
        </select>
      {/if}
      <span class="spacer"></span>
      {#if conv}
        {#if confirmDelete}
          <span class="small">{t.chat.deleteChatConfirm}</span>
          <button class="btn danger" onclick={() => { app.deleteConversation(conv.id); confirmDelete = false; }}>{t.models.delete}</button>
          <button class="btn ghost" onclick={() => (confirmDelete = false)}>{t.model.back}</button>
        {:else}
          <button class="icon-btn" onclick={() => (confirmDelete = true)} aria-label={t.chat.deleteChat} title={t.chat.deleteChat}><Icon name="trash" /></button>
        {/if}
      {/if}
    </header>

    {#if !hasModel}
      <div class="center">
        <div class="card empty-card">
          <h1>{t.chat.noModelTitle}</h1>
          <p class="muted">{t.chat.noModel}</p>
          <button class="btn big" onclick={() => app.go({ name: "use_case" })}>{t.chat.findModel}</button>
        </div>
      </div>
    {:else}
      <div class="messages" bind:this={list} onscroll={onScroll} role="log" aria-live="polite" aria-busy={app.generating} aria-label={t.chat.conversations}>
        {#if !conv || conv.messages.length === 0}
          {#if !stream}
            <div class="empty">
              <h1>{t.chat.emptyTitle}</h1>
              <div class="prompts">
                {#each t.chat.empty[useCase] as p (p)}
                  <button class="prompt" onclick={() => send(p)}>{p}</button>
                {/each}
              </div>
            </div>
          {/if}
        {/if}
        {#if dropped > 0}
          <p class="notice small" role="status"><Icon name="info" size={16} /> {t.chat.dropped(dropped)}</p>
        {/if}
        {#each conv?.messages ?? [] as m, i (m.id + i)}
          <MessageView
            message={m}
            canRegenerate={i === lastAssistant && i === (conv?.messages.length ?? 0) - 1 && !app.generating}
            onregenerate={() => composer?.regenerate()}
          />
        {/each}
        {#if stream}
          <MessageView
            message={{ role: "assistant", content: stream.answer, thinking: stream.thinking || null, attachments: [], stats: null, stopped: false }}
            streaming
          />
        {/if}
        {#if app.loadingFraction !== null && app.generating}
          <p class="notice small" role="status">{t.chat.loading(Math.round(app.loadingFraction * 100))}</p>
        {/if}
        {#if app.chatError}
          <div class="banner bad error" role="alert">
            <Icon name="info" />
            <div>
              <p>{errorText()}</p>
              {#if app.chatError.kind !== "message_too_long"}
                <button class="btn secondary" onclick={() => composer?.regenerate()}><Icon name="refresh" /> {t.chat.regenerate}</button>
              {/if}
            </div>
          </div>
        {/if}
      </div>
      <Composer bind:this={composer} {canThink} />
    {/if}
  </main>
</div>

<style>
  .layout {
    display: grid;
    grid-template-columns: 280px 1fr;
    height: 100%;
  }
  .sidebar {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
    padding: calc(0.9rem + var(--safe-top)) 0.75rem calc(0.75rem + var(--safe-bottom));
    background: var(--surface-2);
    border-right: 1px solid var(--border);
    min-height: 0;
  }
  .side-top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 0.25rem;
  }
  .close {
    display: none;
  }
  .new {
    justify-content: flex-start;
  }
  .convs {
    flex: 1;
    overflow-y: auto;
    min-height: 0;
  }
  .convs ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .conv {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    width: 100%;
    min-height: var(--tap);
    padding: 0.5rem 0.75rem;
    border: none;
    border-radius: var(--radius-s);
    background: none;
    color: var(--text);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .conv:hover {
    background: var(--surface-3);
  }
  .conv.active {
    background: var(--surface);
    box-shadow: var(--shadow);
  }
  .conv .title {
    display: block;
    width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 0.95rem;
  }
  .side-bottom {
    display: flex;
    flex-direction: column;
  }
  .side-bottom .btn {
    justify-content: flex-start;
  }
  .scrim {
    display: none;
  }
  .main {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    height: 100%;
  }
  .bar {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: calc(0.4rem + var(--safe-top)) 0.75rem 0.4rem;
    border-bottom: 1px solid var(--border);
    min-height: 56px;
  }
  .menu {
    display: none;
  }
  .model-pick {
    width: auto;
    max-width: 60vw;
    font-weight: 600;
    border-color: transparent;
    background: transparent;
  }
  .model-pick:hover {
    background: var(--surface-2);
  }
  .spacer {
    flex: 1;
  }
  .messages {
    flex: 1;
    overflow-y: auto;
    padding: 1rem;
    min-height: 0;
  }
  .empty {
    max-width: 760px;
    margin: 12vh auto 0;
    text-align: center;
  }
  .prompts {
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: 0.6rem;
    margin-top: 1.5rem;
  }
  .prompt {
    border: 1px solid var(--border);
    background: var(--surface);
    color: var(--text);
    font: inherit;
    border-radius: 999px;
    padding: 0.55rem 1rem;
    min-height: var(--tap);
    cursor: pointer;
  }
  .prompt:hover {
    border-color: var(--accent);
  }
  .notice {
    max-width: 760px;
    margin: 0.5rem auto;
    display: flex;
    gap: 0.4rem;
    align-items: center;
    color: var(--text-2);
  }
  .error {
    max-width: 760px;
    margin: 0.75rem auto;
  }
  .center {
    flex: 1;
    display: grid;
    place-items: center;
    padding: 1rem;
  }
  .empty-card {
    max-width: 420px;
    text-align: center;
  }

  /* Narrow screens: the conversation list becomes a drawer. */
  @media (max-width: 800px) {
    .layout {
      grid-template-columns: 1fr;
    }
    .sidebar {
      position: fixed;
      inset: 0 auto 0 0;
      width: min(86vw, 320px);
      z-index: 20;
      transform: translateX(-100%);
      transition: transform 0.2s ease;
      box-shadow: var(--shadow);
    }
    .drawer-open .sidebar {
      transform: none;
    }
    .drawer-open .scrim {
      display: block;
      position: fixed;
      inset: 0;
      z-index: 10;
      border: none;
      background: rgb(0 0 0 / 0.35);
    }
    .menu,
    .close {
      display: inline-flex;
    }
  }
</style>
