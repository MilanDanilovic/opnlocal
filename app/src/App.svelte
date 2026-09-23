<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";

  let report = $state("");

  async function probe() {
    report = JSON.stringify(await invoke("probe"), null, 2);
  }
</script>

<main>
  <h1>opnlocal</h1>
  <button onclick={probe}>Find what my device can run</button>
  <pre>{report}</pre>
</main>
