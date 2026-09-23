import { defineConfig } from "@playwright/test";

// Real-app tests: attach to the built desktop app (see tests-real/). Windows only (WebView2).
export default defineConfig({
  testDir: "tests-real",
  timeout: 15 * 60_000,
  workers: 1,
  reporter: [["list"]],
});
