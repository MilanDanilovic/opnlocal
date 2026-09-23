// End-to-end UI journeys against the in-browser test backend. Each screen is also checked with
// axe (WCAG 2.2 A/AA rules) and screenshotted into test-results/shots for visual review.
import { expect, test, type Page } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";

async function shot(page: Page, name: string) {
  await page.screenshot({ path: `test-results/shots/${test.info().project.name}-${name}.png`, fullPage: true });
}

async function accessible(page: Page) {
  const results = await new AxeBuilder({ page }).withTags(["wcag2a", "wcag2aa", "wcag21aa", "wcag22aa"]).analyze();
  const serious = results.violations.filter((v) => v.impact === "serious" || v.impact === "critical");
  expect(serious.map((v) => `${v.id}: ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`)).toEqual([]);
}

test("first run: find, download, measure, chat", async ({ page }) => {
  await page.goto("/?scenario=gaming-pc");
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("AI that runs on your own device");
  await expect(page.getByText("Your chats never leave this device")).toBeVisible();
  await accessible(page);
  await shot(page, "01-welcome");

  await page.getByRole("button", { name: "Find what my device can run" }).click();
  await expect(page.getByRole("heading", { name: "Here's what we found" })).toBeVisible();
  await expect(page.getByText("32 GB of memory")).toBeVisible();
  await expect(page.getByText(/AMD Radeon RX 7800 XT/)).toBeVisible();
  await accessible(page);
  await shot(page, "02-device");

  await page.getByRole("button", { name: "Continue" }).click();
  await expect(page.getByRole("heading", { name: "What would you like to do?" })).toBeVisible();
  await accessible(page);
  await shot(page, "03-use-case");

  await page.getByRole("button", { name: /Coding/ }).click();
  await expect(page.getByRole("heading", { name: /Good picks for coding/ })).toBeVisible();
  await expect(page.getByText("Recommended", { exact: true })).toBeVisible();
  // Estimates never show a speed; only fit.
  await expect(page.getByText(/words per second/)).toHaveCount(0);
  await accessible(page);
  await shot(page, "04-recommendations");

  await page.getByRole("button", { name: "See all models" }).click();
  await shot(page, "05-all-models");

  await page.getByRole("article").first().getByRole("button", { name: "Set up" }).click();
  await expect(page.getByRole("button", { name: /Download/ })).toBeVisible();
  await accessible(page);
  await shot(page, "06-model-details");

  await page.getByRole("button", { name: /Download/ }).click();
  await expect(page.getByRole("heading", { name: /Setting up/ })).toBeVisible();
  await expect(page.getByText(/ of /).first()).toBeVisible();
  await shot(page, "07-downloading");

  await expect(page.getByRole("heading", { name: "Measured on this device" })).toBeVisible({ timeout: 20_000 });
  await expect(page.getByText(/Replies at about \d+ words per second/)).toBeVisible();
  await accessible(page);
  await shot(page, "08-benchmark");

  await page.getByRole("button", { name: "Start chatting" }).click();
  await expect(page.getByRole("heading", { name: "What can I help with?" })).toBeVisible();
  await accessible(page);
  await shot(page, "09-chat-empty");

  await page.getByLabel("Message").fill("How do I start a habit?");
  await page.getByRole("button", { name: "Send" }).click();
  await expect(page.getByRole("button", { name: "Stop" })).toBeVisible();
  await expect(page.getByText("Want me to make a checklist for you?")).toBeVisible({ timeout: 15_000 });
  await expect(page.locator(".md code.hljs")).toBeVisible();
  await expect(page.getByRole("button", { name: "Try again" })).toBeVisible();
  await accessible(page);
  await shot(page, "10-chat-reply");
});

test("license terms must be accepted before a custom-license model downloads", async ({ page }) => {
  await page.goto("/?scenario=phone");
  await page.getByRole("button", { name: "Find what my device can run" }).click();
  await page.getByRole("button", { name: "Continue" }).click();
  await page.getByRole("button", { name: /Everyday help/ }).click();
  await page.getByRole("button", { name: "See all models" }).click();
  await page.getByRole("article").filter({ hasText: "LFM2.5 2.6B" }).getByRole("button", { name: /Set up|Continue/ }).click();
  await expect(page.getByRole("heading", { name: "This model has its own terms" })).toBeVisible();
  const download = page.getByRole("button", { name: /Download/ });
  await expect(download).toBeDisabled();
  await page.getByLabel(/I've read the terms/).check();
  await expect(download).toBeEnabled();
  await accessible(page);
  await shot(page, "11-license");
});

test("old phone gets a clear unsupported message instead of a crash", async ({ page }) => {
  await page.goto("/?scenario=old-phone");
  await page.getByRole("button", { name: "Find what my device can run" }).click();
  await expect(page.getByText(/This processor is too old/)).toBeVisible();
  await expect(page.getByRole("button", { name: "Continue" })).toHaveCount(0);
  await accessible(page);
  await shot(page, "12-cpu-too-old");
});

test("device with too little memory is told so plainly", async ({ page }) => {
  await page.goto("/?scenario=tiny-phone");
  await page.getByRole("button", { name: "Find what my device can run" }).click();
  await page.getByRole("button", { name: "Continue" }).click();
  await page.getByRole("button", { name: /Everyday help/ }).click();
  await expect(page.getByRole("heading", { name: "This device doesn't have enough memory" })).toBeVisible();
  await accessible(page);
  await shot(page, "13-not-enough-memory");
});

test("low disk space blocks the download and says how much to free", async ({ page }) => {
  await page.goto("/?scenario=low-disk");
  await page.getByRole("button", { name: "Find what my device can run" }).click();
  await page.getByRole("button", { name: "Continue" }).click();
  await page.getByRole("button", { name: /Everyday help/ }).click();
  await expect(page.getByText(/Not enough space: free up/).first()).toBeVisible();
  await shot(page, "14-low-disk");
});

test("losing the connection mid-download keeps progress and offers a retry", async ({ page }) => {
  await page.goto("/?scenario=offline");
  await page.getByRole("button", { name: "Find what my device can run" }).click();
  await page.getByRole("button", { name: "Continue" }).click();
  await page.getByRole("button", { name: /Everyday help/ }).click();
  await page.getByRole("article").first().getByRole("button", { name: "Set up" }).click();
  await page.getByRole("button", { name: /Download/ }).click();
  await expect(page.getByText("The download stopped")).toBeVisible({ timeout: 10_000 });
  await expect(page.getByText(/What's already downloaded is kept/)).toBeVisible();
  await expect(page.getByRole("button", { name: "Try again" })).toBeVisible();
  await accessible(page);
  await shot(page, "15-download-offline");
});

test("cancel asks first, then returns", async ({ page }) => {
  await page.goto("/?scenario=laptop");
  await page.getByRole("button", { name: "Find what my device can run" }).click();
  await page.getByRole("button", { name: "Continue" }).click();
  await page.getByRole("button", { name: /Writing/ }).click();
  await page.getByRole("article").first().getByRole("button", { name: "Set up" }).click();
  await page.getByRole("button", { name: /Download/ }).click();
  await page.getByRole("button", { name: "Cancel download" }).click();
  await expect(page.getByText("Cancel and delete what's been downloaded so far?")).toBeVisible();
  await page.getByRole("alertdialog").getByRole("button", { name: "Cancel download" }).click();
  await expect(page.getByRole("heading", { name: /Setting up/ })).toHaveCount(0, { timeout: 5_000 });
});

test("returning user lands in chat with history, thinking and stop", async ({ page }) => {
  await page.goto("/?scenario=returning");
  await expect(page.getByText("Here's an easy week:")).toBeVisible();
  await expect(page.locator(".md table")).toBeVisible();
  await accessible(page);
  await shot(page, "16-chat-history");

  await page.getByLabel("Think harder").check();
  await page.getByLabel("Message").fill("Make it vegetarian");
  await page.getByRole("button", { name: "Send" }).click();
  await expect(page.getByText("Thinking…").first()).toBeVisible();
  await page.getByRole("button", { name: "Stop" }).click();
  await expect(page.getByText("Stopped")).toBeVisible({ timeout: 10_000 });
  await expect(page.getByText("Show thinking")).toBeVisible();
  await shot(page, "17-chat-stopped");
});

test("settings explain every network connection and keep technical controls in Advanced", async ({ page }) => {
  await page.goto("/?scenario=returning");
  if (test.info().project.name === "phone") await page.getByRole("button", { name: "Menu" }).click();
  await page.getByRole("button", { name: "Settings" }).click();
  await expect(page.getByText("Model downloads")).toBeVisible();
  await expect(page.getByText("New-model check")).toBeVisible();
  await expect(page.getByLabel("Check for new models automatically")).toBeChecked();
  await expect(page.getByLabel(/Context length/)).toBeHidden();
  await page.getByText("Advanced").click();
  await expect(page.getByLabel(/Context length/)).toBeVisible();
  await accessible(page);
  await shot(page, "18-settings");
});
