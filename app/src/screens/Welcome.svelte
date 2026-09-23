<script lang="ts">
  import { app } from "../lib/store.svelte";
  import { t } from "../lib/strings";
  import { focusOnMount } from "../lib/focus";
  import Icon from "../components/Icon.svelte";
  import Logo from "../components/Logo.svelte";
</script>

<main class="page">
  <div class="inner welcome">
    <Logo size={48} />
    <h1 use:focusOnMount>{t.welcome.title}</h1>
    <p class="lead">{t.welcome.lead}</p>
    <button class="btn big" onclick={() => app.go({ name: "device" })}>
      {t.welcome.start}
    </button>

    <section class="card privacy" aria-labelledby="privacy-title">
      <div class="head">
        <span class="badge"><Icon name="lock" /></span>
        <h2 id="privacy-title">{t.welcome.privacyTitle}</h2>
      </div>
      <ul>
        {#each t.welcome.privacyPoints as p (p)}
          <li>{p}</li>
        {/each}
      </ul>
      <details>
        <summary>{t.welcome.privacyMore}</summary>
        <dl>
          {#each t.settings.connections as [what, detail] (what)}
            <dt>{what}</dt>
            <dd>{detail}</dd>
          {/each}
        </dl>
        <p class="small muted">{t.settings.never}</p>
      </details>
    </section>
  </div>
</main>

<style>
  .welcome {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 1rem;
    padding-top: clamp(1rem, 8vh, 5rem);
  }
  h1 {
    margin: 0.5rem 0 0;
    max-width: 18ch;
  }
  .lead {
    font-size: 1.1rem;
    color: var(--text-2);
    max-width: 44ch;
  }
  .privacy {
    margin-top: 1.5rem;
    width: 100%;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 0.75rem;
  }
  .head h2 {
    margin: 0;
  }
  .badge {
    display: grid;
    place-items: center;
    width: 40px;
    height: 40px;
    border-radius: 12px;
    background: var(--accent-soft);
    color: var(--accent);
  }
  ul {
    margin: 0.75rem 0;
    padding-left: 1.2rem;
    color: var(--text-2);
  }
  li + li {
    margin-top: 0.35rem;
  }
  summary {
    cursor: pointer;
    min-height: var(--tap);
    display: flex;
    align-items: center;
    font-weight: 600;
  }
  dt {
    font-weight: 600;
    margin-top: 0.5rem;
  }
  dd {
    margin: 0.1rem 0 0;
    color: var(--text-2);
  }
</style>
