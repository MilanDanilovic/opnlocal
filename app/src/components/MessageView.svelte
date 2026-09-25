<script lang="ts">
  import { t } from "../lib/strings";
  import { renderMarkdown } from "../lib/markdown";
  import { countWords } from "../lib/format";
  import Icon from "./Icon.svelte";
  import type { Message } from "../lib/bindings/Message";

  let {
    message,
    streaming = false,
    canRegenerate = false,
    onregenerate,
  }: {
    message: Pick<Message, "role" | "content" | "thinking" | "attachments" | "stats" | "stopped">;
    streaming?: boolean;
    canRegenerate?: boolean;
    onregenerate?: () => void;
  } = $props();

  let copied = $state(false);
  const html = $derived(message.role === "assistant" ? renderMarkdown(message.content) : "");

  async function copy() {
    await navigator.clipboard.writeText(message.content);
    copied = true;
    setTimeout(() => (copied = false), 1500);
  }
</script>

{#if message.role === "user"}
  <div class="msg user">
    {#each message.attachments as a (a.name)}
      <p class="chip">
        <Icon name={a.image ? "image" : "doc"} size={16} />
        {a.image ? t.chat.attachmentImage(a.name, countWords(a.text)) : t.chat.attachmentWords(a.name, countWords(a.text))}
      </p>
    {/each}
    {#if message.content}<div class="bubble">{message.content}</div>{/if}
  </div>
{:else}
  <div class="msg assistant">
    {#if message.thinking || (streaming && !message.content && message.thinking !== null)}
      <details class="thinking" open={streaming && !message.content}>
        <summary>
          <Icon name="think" size={16} />
          {streaming && !message.content ? t.chat.thinking : t.chat.showThinking}
        </summary>
        <p>{message.thinking}</p>
      </details>
    {/if}
    {#if message.content}
      <div class="md">{@html html}</div>
    {:else if streaming && !message.thinking}
      <p class="typing" aria-label={t.chat.thinking}><span></span><span></span><span></span></p>
    {/if}
    {#if !streaming}
      <div class="actions">
        <button class="icon-btn" onclick={copy} aria-label={copied ? t.chat.copied : t.chat.copy} title={t.chat.copy}>
          <Icon name={copied ? "check" : "copy"} size={18} />
        </button>
        {#if canRegenerate}
          <button class="icon-btn" onclick={onregenerate} aria-label={t.chat.regenerate} title={t.chat.regenerate}>
            <Icon name="refresh" size={18} />
          </button>
        {/if}
        {#if message.stopped}<span class="muted small">{t.chat.stopped}</span>{/if}
        {#if message.stats}
          <span class="muted small stats">{t.chat.stats(message.stats.words, message.stats.seconds)}</span>
        {/if}
      </div>
    {/if}
  </div>
{/if}

<style>
  .msg {
    max-width: 760px;
    width: 100%;
    margin: 0 auto;
    padding: 0.5rem 0;
  }
  .user {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 0.35rem;
  }
  .bubble {
    background: var(--accent-soft);
    border-radius: var(--radius-l) var(--radius-l) 6px var(--radius-l);
    padding: 0.65rem 1rem;
    max-width: min(85%, 620px);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .chip {
    display: inline-flex;
    gap: 0.35rem;
    align-items: center;
    margin: 0;
    padding: 0.3rem 0.7rem;
    border-radius: 999px;
    background: var(--surface-2);
    border: 1px solid var(--border);
    font-size: 0.85rem;
  }
  .thinking {
    border-left: 3px solid var(--border);
    padding-left: 0.75rem;
    margin-bottom: 0.5rem;
    color: var(--text-2);
  }
  .thinking summary {
    cursor: pointer;
    display: inline-flex;
    gap: 0.4rem;
    align-items: center;
    min-height: 36px;
    font-size: 0.9rem;
  }
  .thinking p {
    white-space: pre-wrap;
    font-size: 0.9rem;
  }
  .md {
    overflow-wrap: anywhere;
  }
  .md :global(p) {
    margin: 0 0 0.8em;
  }
  .md :global(pre) {
    margin: 0;
    padding: 0.85rem 1rem;
    overflow-x: auto;
    font: 0.88rem/1.5 var(--mono);
  }
  .md :global(code) {
    font-family: var(--mono);
    font-size: 0.92em;
  }
  .md :global(:not(pre) > code) {
    background: var(--surface-2);
    padding: 0.1em 0.35em;
    border-radius: 5px;
  }
  .md :global(.code) {
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    margin: 0 0 0.9em;
    overflow: hidden;
  }
  .md :global(.code-bar) {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 0.2rem 0.4rem 0.2rem 1rem;
    border-bottom: 1px solid var(--border);
    font-size: 0.8rem;
    color: var(--text-2);
  }
  .md :global(.copy-code) {
    border: none;
    background: none;
    color: var(--text-2);
    font: inherit;
    padding: 0.4rem 0.7rem;
    min-height: 36px;
    cursor: pointer;
    border-radius: 6px;
  }
  .md :global(.copy-code:hover) {
    background: var(--surface-3);
    color: var(--text);
  }
  .md :global(table) {
    border-collapse: collapse;
    margin: 0 0 0.9em;
    display: block;
    overflow-x: auto;
  }
  .md :global(th),
  .md :global(td) {
    border: 1px solid var(--border);
    padding: 0.4rem 0.7rem;
    text-align: left;
  }
  .md :global(ul),
  .md :global(ol) {
    padding-left: 1.4rem;
    margin: 0 0 0.8em;
  }
  .md :global(blockquote) {
    margin: 0 0 0.8em;
    padding-left: 0.9rem;
    border-left: 3px solid var(--border);
    color: var(--text-2);
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 0.15rem;
    margin-left: -0.6rem;
    min-height: var(--tap);
  }
  .stats {
    margin-left: 0.5rem;
  }
  .typing {
    display: inline-flex;
    gap: 5px;
    padding: 0.6rem 0;
  }
  .typing span {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--text-2);
    animation: blink 1.2s infinite;
  }
  .typing span:nth-child(2) {
    animation-delay: 0.2s;
  }
  .typing span:nth-child(3) {
    animation-delay: 0.4s;
  }
  @keyframes blink {
    0%,
    80%,
    100% {
      opacity: 0.25;
    }
    40% {
      opacity: 1;
    }
  }
</style>
