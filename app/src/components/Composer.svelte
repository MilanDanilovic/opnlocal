<script lang="ts">
  import { app } from "../lib/store.svelte";
  import { t } from "../lib/strings";
  import { api, asEngineError, backend } from "../lib/api";
  import { countWords } from "../lib/format";
  import Icon from "./Icon.svelte";
  import type { Attachment } from "../lib/bindings/Attachment";

  let { canThink }: { canThink: boolean } = $props();

  let text = $state("");
  let attachments = $state<Attachment[]>([]);
  let attachError = $state<string | null>(null);
  let attaching = $state(false);
  let thinkHarder = $state(app.conversation?.think_harder ?? false);
  let area: HTMLTextAreaElement;

  const touch = typeof matchMedia !== "undefined" && matchMedia("(pointer: coarse)").matches;
  const canSend = $derived((text.trim().length > 0 || attachments.length > 0) && !app.generating);

  function autosize() {
    area.style.height = "auto";
    area.style.height = `${Math.min(area.scrollHeight, 220)}px`;
  }

  async function send() {
    if (!canSend) return;
    const msg = text.trim();
    const files = attachments;
    text = "";
    attachments = [];
    queueMicrotask(autosize);
    await app.send(msg, files, thinkHarder);
  }

  function onKey(e: KeyboardEvent) {
    // Desktop: Enter sends, Shift+Enter adds a line. Touch keyboards: Enter adds a line.
    if (e.key === "Enter" && !e.shiftKey && !touch && !e.isComposing) {
      e.preventDefault();
      send();
    }
  }

  async function attach() {
    attachError = null;
    const path = await (await backend()).pickFile({ documents: true });
    if (!path) return;
    attaching = true;
    try {
      attachments = [...attachments, await api.attachFile(path)];
    } catch (e) {
      const err = asEngineError(e);
      attachError = "message" in err ? String(err.message) : t.errors.generic;
    } finally {
      attaching = false;
    }
  }

  export function regenerate() {
    app.regenerate(thinkHarder);
  }
</script>

<form class="composer" onsubmit={(e) => { e.preventDefault(); send(); }}>
  {#if attachments.length || attachError}
    <div class="attachments">
      {#each attachments as a, i (a.name + i)}
        <span class="chip">
          <Icon name="doc" size={16} />
          {t.chat.attachmentWords(a.name, countWords(a.text))}
          <button type="button" class="x" aria-label={t.chat.removeAttachment(a.name)} onclick={() => (attachments = attachments.filter((_, j) => j !== i))}>
            <Icon name="x" size={14} />
          </button>
        </span>
      {/each}
      {#if attachError}<span class="err small" role="alert">{attachError}</span>{/if}
    </div>
  {/if}
  <div class="box">
    <button type="button" class="icon-btn" onclick={attach} disabled={attaching || app.generating} aria-label={t.chat.attach} title={t.chat.attach}>
      <Icon name="attach" />
    </button>
    <label class="visually-hidden" for="composer-input">{t.chat.placeholder}</label>
    <textarea
      id="composer-input"
      bind:this={area}
      bind:value={text}
      oninput={autosize}
      onkeydown={onKey}
      rows="1"
      placeholder={t.chat.placeholder}
      enterkeyhint={touch ? "enter" : "send"}
    ></textarea>
    {#if app.generating}
      <button type="button" class="send stop" onclick={() => app.stop()} aria-label={t.chat.stop} title={t.chat.stop}>
        <Icon name="stop" />
      </button>
    {:else}
      <button type="submit" class="send" disabled={!canSend} aria-label={t.chat.send} title={t.chat.send}>
        <Icon name="send" />
      </button>
    {/if}
  </div>
  <div class="below">
    {#if canThink}
      <label class="think" title={t.chat.thinkHarderHint}>
        <input type="checkbox" bind:checked={thinkHarder} />
        <Icon name="think" size={16} />
        <span>{t.chat.thinkHarder}</span>
        <span class="muted small hint">{t.chat.thinkHarderHint}</span>
      </label>
    {/if}
    <span class="muted small local"><Icon name="lock" size={14} /> {t.chat.local}</span>
  </div>
</form>

<style>
  .composer {
    max-width: 800px;
    width: 100%;
    margin: 0 auto;
    padding: 0.5rem 1rem calc(0.75rem + var(--safe-bottom));
  }
  .box {
    display: flex;
    align-items: flex-end;
    gap: 0.25rem;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 24px;
    padding: 0.3rem;
    box-shadow: var(--shadow);
  }
  .box:focus-within {
    border-color: var(--accent);
  }
  textarea {
    flex: 1;
    resize: none;
    border: none;
    background: transparent;
    padding: 0.6rem 0.25rem;
    min-height: var(--tap);
    max-height: 220px;
    line-height: 1.45;
  }
  textarea:focus-visible {
    outline: none;
  }
  .send {
    display: grid;
    place-items: center;
    width: var(--tap);
    height: var(--tap);
    border-radius: 50%;
    border: none;
    background: var(--accent);
    color: var(--accent-contrast);
    cursor: pointer;
    flex: none;
  }
  .send:disabled {
    background: var(--surface-3);
    color: var(--text-2);
    cursor: default;
  }
  .send.stop {
    background: var(--text);
    color: var(--bg);
  }
  .attachments {
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem;
    margin-bottom: 0.4rem;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    padding: 0.2rem 0.25rem 0.2rem 0.7rem;
    border-radius: 999px;
    background: var(--surface-2);
    border: 1px solid var(--border);
    font-size: 0.85rem;
  }
  .x {
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    border: none;
    border-radius: 50%;
    background: none;
    color: var(--text-2);
    cursor: pointer;
  }
  .err {
    color: var(--bad);
  }
  .below {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    flex-wrap: wrap;
    padding: 0.35rem 0.5rem 0;
  }
  .think {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    min-height: 36px;
    font-weight: 500;
    font-size: 0.92rem;
    cursor: pointer;
  }
  .think input {
    width: 18px;
    height: 18px;
    accent-color: var(--accent);
  }
  .local {
    display: inline-flex;
    gap: 0.3rem;
    align-items: center;
  }
  @media (max-width: 520px) {
    .hint {
      display: none;
    }
  }
</style>
