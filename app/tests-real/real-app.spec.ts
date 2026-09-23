// Drives the real Windows app (real engine, real llama.cpp, real downloads) through Playwright,
// attached to its WebView2 over the Chrome DevTools protocol. Opt-in and slow: it downloads a
// ~0.8 GB model. Run with:  npx playwright test -c playwright.real.config.ts
import { expect, test, chromium, type Browser, type Page } from "@playwright/test";
import { spawn, type ChildProcess } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const EXE = process.env.OPNLOCAL_EXE ?? "C:\\ot\\release\\opnlocal.exe";
const PORT = 9333;
let app: ChildProcess;
let browser: Browser;
let page: Page;
let dataDir: string;

async function connect(): Promise<Page> {
  for (let i = 0; i < 60; i++) {
    try {
      browser = await chromium.connectOverCDP(`http://127.0.0.1:${PORT}`);
      const p = browser.contexts()[0]?.pages()[0];
      if (p) return p;
    } catch {
      // app still starting
    }
    await new Promise((r) => setTimeout(r, 500));
  }
  throw new Error("could not attach to the app's web view");
}

function shot(name: string) {
  return page.screenshot({ path: `test-results/real/${name}.png` });
}

test.beforeAll(async () => {
  dataDir = mkdtempSync(join(tmpdir(), "opnlocal-real-"));
  app = spawn(EXE, [], {
    env: {
      ...process.env,
      OPNLOCAL_DATA_DIR: dataDir,
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${PORT}`,
    },
    stdio: "ignore",
  });
  page = await connect();
  await page.setViewportSize({ width: 1180, height: 800 }).catch(() => {});
});

test.afterAll(async () => {
  await browser?.close().catch(() => {});
  app?.kill();
  await new Promise((r) => setTimeout(r, 1500));
  rmSync(dataDir, { recursive: true, force: true });
});

test("real device: detect, recommend, download, measure, chat", async () => {
  test.setTimeout(15 * 60_000);
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("AI that runs on your own device");
  await page.getByRole("button", { name: "Find what my device can run" }).click();

  // Real hardware detection through llama.cpp's device list.
  await expect(page.getByRole("heading", { name: "Here's what we found" })).toBeVisible({ timeout: 30_000 });
  await shot("device");
  const facts = await page.locator(".facts").innerText();
  console.log("DEVICE:\n" + facts);
  expect(facts).toMatch(/GB of memory/);

  await page.getByRole("button", { name: "Continue" }).click();
  await page.getByRole("button", { name: /Everyday help/ }).click();
  await expect(page.getByRole("heading", { name: /Good picks/ })).toBeVisible();
  await shot("recommendations");
  console.log("PICKS:\n" + (await page.locator(".picks").innerText()));

  // The smallest catalog model keeps the download short.
  await page.getByRole("button", { name: "See all models" }).click();
  await shot("all-models");
  const tiny = page.getByRole("article").filter({ hasText: "Qwen3.5 0.8B" });
  await tiny.getByRole("button", { name: /Set up|Continue/ }).click();
  await page.getByRole("button", { name: /Download/ }).click();
  await expect(page.getByRole("heading", { name: /Setting up Qwen3.5 0.8B/ })).toBeVisible();
  await page.waitForTimeout(3000);
  await shot("downloading");

  // Real download (sha256-verified), then the real benchmark.
  await expect(page.getByRole("heading", { name: "Measured on this device" })).toBeVisible({ timeout: 10 * 60_000 });
  await shot("benchmark");
  console.log("BENCH:\n" + (await page.locator(".result").innerText()));
  await expect(page.getByText(/Replies at about [\d.]+ words per second/)).toBeVisible();

  await page.getByRole("button", { name: "Start chatting" }).click();
  await page.getByLabel("Message").fill("In one sentence: why is the sky blue?");
  await page.getByRole("button", { name: "Send" }).click();
  const reply = page.locator(".msg.assistant .md").last();
  await expect(page.getByRole("button", { name: "Send" })).toBeVisible({ timeout: 120_000 });
  const text = await reply.innerText();
  console.log("REPLY: " + text);
  expect(text.length).toBeGreaterThan(10);
  await shot("chat");

  // A second turn in the same conversation.
  await page.getByLabel("Message").fill("Now say it for a five-year-old.");
  await page.getByRole("button", { name: "Send" }).click();
  await expect(page.locator(".msg.assistant")).toHaveCount(2, { timeout: 120_000 });
  await expect(page.getByRole("button", { name: "Send" })).toBeVisible({ timeout: 120_000 });
  await shot("chat-2");

  // Stop a long reply.
  await page.getByLabel("Message").fill("Write a 600-word story about a lighthouse keeper.");
  await page.getByRole("button", { name: "Send" }).click();
  await page.waitForTimeout(1500);
  await page.getByRole("button", { name: "Stop" }).click();
  await expect(page.getByText("Stopped")).toBeVisible({ timeout: 30_000 });
  await shot("chat-stopped");
});
