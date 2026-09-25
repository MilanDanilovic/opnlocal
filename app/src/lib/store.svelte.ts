// App-wide state and navigation. Screens read from `app` and call the actions below; engine
// events (downloads, streaming replies, benchmark stages) update it as they arrive.

import { api, asEngineError, backend, type EngineError, type EngineEvent } from "./api";
import type { AppState } from "./bindings/AppState";
import type { Attachment } from "./bindings/Attachment";
import type { BenchStage } from "./bindings/BenchStage";
import type { BenchmarkResult } from "./bindings/BenchmarkResult";
import type { CatalogModel } from "./bindings/CatalogModel";
import type { Conversation } from "./bindings/Conversation";
import type { DeviceInfo } from "./bindings/DeviceInfo";
import type { DownloadError } from "./bindings/DownloadError";
import type { Progress } from "./bindings/Progress";
import type { Recommendations } from "./bindings/Recommendations";
import type { Settings } from "./bindings/Settings";
import type { UseCase } from "./bindings/UseCase";

export type Screen =
  | { name: "welcome" }
  | { name: "device" }
  | { name: "use_case" }
  | { name: "recommend" }
  | { name: "model"; id: string }
  | { name: "download"; id: string }
  | { name: "benchmark"; id: string }
  | { name: "chat"; conversationId: string | null }
  | { name: "models" }
  | { name: "settings" };

export interface DownloadState {
  modelId: string;
  /** The model's image encoder rather than the model itself. */
  vision: boolean;
  progress: Progress | null;
  status: "running" | "done" | "failed" | "cancelled";
  error: DownloadError | EngineError | null;
}

export interface Streaming {
  conversationId: string | null;
  messageId: string | null;
  thinking: string;
  answer: string;
  droppedMessages: number;
  /** Attached documents were cut down to the parts relevant to the question. */
  partialDocuments: boolean;
}

class App {
  state = $state<AppState | null>(null);
  device = $state<DeviceInfo | null>(null);
  recs = $state<Recommendations | null>(null);
  screen = $state<Screen>({ name: "welcome" });
  download = $state<DownloadState | null>(null);
  benchStage = $state<BenchStage | null>(null);
  loadingFraction = $state<number | null>(null);
  /** Progress through a long prompt (a document) before the reply starts. */
  readingFraction = $state<number | null>(null);
  importFraction = $state<number | null>(null);
  conversation = $state<Conversation | null>(null);
  streaming = $state<Streaming | null>(null);
  chatError = $state<EngineError | null>(null);
  error = $state<EngineError | null>(null);
  online = $state(typeof navigator === "undefined" ? true : navigator.onLine);

  get settings(): Settings | null {
    return this.state?.settings ?? null;
  }

  model(id: string | null | undefined): CatalogModel | null {
    return this.state?.models.find((m) => m.id === id) ?? null;
  }

  /** Display name for any model id (catalog or imported). */
  modelName(id: string | null | undefined): string {
    if (!id) return "";
    return this.model(id)?.name ?? this.state?.imported.find((m) => m.id === id)?.name ?? id;
  }

  isInstalled(id: string): boolean {
    return this.state?.installed.includes(id) ?? false;
  }

  partialBytes(id: string): number {
    return this.state?.partial_downloads.find((p) => p.model_id === id)?.bytes ?? 0;
  }

  benchmark(id: string): BenchmarkResult | null {
    return this.state?.benchmarks[id] ?? null;
  }

  async init() {
    window.addEventListener("online", () => (this.online = true));
    window.addEventListener("offline", () => (this.online = false));
    window.addEventListener("popstate", (e) => {
      if (e.state?.screen) this.screen = e.state.screen;
    });
    (await backend()).listen((e) => this.onEvent(e));
    await this.refresh();
    const s = this.state!;
    const start: Screen = s.settings.onboarding_done
      ? { name: "chat", conversationId: s.conversations[0]?.id ?? null }
      : { name: "welcome" };
    history.replaceState({ screen: $state.snapshot(start) }, "");
    this.screen = start;
    if (start.name === "chat" && start.conversationId) await this.openConversation(start.conversationId);
  }

  async refresh() {
    this.state = await api.state();
  }

  go(screen: Screen) {
    this.screen = screen;
    history.pushState({ screen: $state.snapshot(screen) }, "");
  }

  back() {
    history.back();
  }

  private onEvent(e: EngineEvent) {
    switch (e.type) {
      case "download_progress":
        if (this.download?.modelId === e.model_id) this.download.progress = e.progress;
        break;
      case "download_finished":
        // The awaiting call in startDownload handles the outcome.
        break;
      case "model_loading":
        this.loadingFraction = e.fraction >= 1 ? null : e.fraction;
        break;
      case "import_progress":
        this.importFraction = e.total > 0 ? e.done / e.total : null;
        break;
      case "benchmark_stage":
        this.benchStage = e.stage;
        break;
      case "chat_reading":
        this.readingFraction = e.fraction >= 1 ? null : e.fraction;
        break;
      case "chat_started":
        this.loadingFraction = null;
        this.readingFraction = null;
        if (this.streaming) {
          this.streaming.messageId = e.message_id;
          this.streaming.conversationId = e.conversation_id;
          this.streaming.droppedMessages = e.dropped_messages;
          this.streaming.partialDocuments = e.partial_documents;
        }
        break;
      case "chat_delta":
        this.readingFraction = null;
        if (this.streaming && this.streaming.messageId === e.message_id) {
          if (e.thinking) this.streaming.thinking += e.thinking;
          if (e.answer) this.streaming.answer += e.answer;
        }
        break;
    }
  }

  async saveSettings(patch: Partial<Settings>) {
    const next = { ...$state.snapshot(this.state!.settings), ...patch } as Settings;
    const saved = await api.saveSettings(next);
    this.state!.settings = saved;
  }

  // ---- onboarding -------------------------------------------------------------------

  async detect() {
    this.device = null;
    this.device = await api.detectDevice();
  }

  async chooseUseCase(useCase: UseCase) {
    await this.saveSettings({ use_case: useCase });
    await this.loadRecommendations();
    this.go({ name: "recommend" });
  }

  async loadRecommendations() {
    const useCase = this.settings?.use_case ?? "everyday";
    this.recs = await api.recommend(useCase);
  }

  // ---- downloads ----------------------------------------------------------------------

  async startDownload(modelId: string, vision = false) {
    this.download = { modelId, vision, progress: null, status: "running", error: null };
    try {
      await (vision ? api.downloadVision(modelId) : api.download(modelId));
      this.download.status = "done";
      await this.refresh();
    } catch (e) {
      const err = asEngineError(e);
      const cancelled = err.kind === "download" && err.error.kind === "cancelled";
      this.download.status = cancelled ? "cancelled" : "failed";
      this.download.error = err.kind === "download" ? err.error : err;
      await this.refresh();
    }
  }

  async cancelDownload() {
    await api.cancelDownload();
  }

  // ---- benchmark ----------------------------------------------------------------------

  async runBenchmark(modelId: string): Promise<BenchmarkResult> {
    this.benchStage = "loading";
    try {
      const r = await api.benchmark(modelId);
      await this.refresh();
      return r;
    } finally {
      this.benchStage = null;
    }
  }

  /** Finishes onboarding with this model and opens a fresh chat. */
  async finishSetup(modelId: string) {
    await this.saveSettings({ onboarding_done: true, active_model: modelId });
    this.conversation = null;
    this.go({ name: "chat", conversationId: null });
  }

  // ---- chat ---------------------------------------------------------------------------

  async openConversation(id: string | null) {
    this.chatError = null;
    this.conversation = id ? await api.conversation(id) : null;
  }

  newChat() {
    this.conversation = null;
    this.chatError = null;
    this.go({ name: "chat", conversationId: null });
  }

  get generating(): boolean {
    return this.streaming !== null;
  }

  async send(text: string, attachments: Attachment[], thinkHarder: boolean) {
    this.chatError = null;
    const conversationId = this.conversation?.id ?? null;
    // Show the user's message right away.
    const optimistic = {
      id: "pending",
      role: "user" as const,
      content: text,
      thinking: null,
      attachments,
      created_at: Date.now() / 1000,
      stats: null,
      stopped: false,
    };
    if (this.conversation) this.conversation.messages.push(optimistic);
    this.streaming = { conversationId, messageId: null, thinking: "", answer: "", droppedMessages: 0, partialDocuments: false };
    try {
      const c = await api.sendMessage(conversationId, text, attachments, thinkHarder);
      this.conversation = c;
      if (!conversationId) history.replaceState({ screen: { name: "chat", conversationId: c.id } }, "");
    } catch (e) {
      this.chatError = asEngineError(e);
      // Reload what was saved (the user message is kept, so "Try again" works).
      const id = this.streaming?.conversationId ?? conversationId;
      if (id) this.conversation = await api.conversation(id).catch(() => this.conversation);
    } finally {
      this.streaming = null;
      this.loadingFraction = null;
      this.readingFraction = null;
      await this.refresh();
    }
  }

  async regenerate(thinkHarder: boolean) {
    const c = this.conversation;
    if (!c) return;
    this.chatError = null;
    while (c.messages.length && c.messages[c.messages.length - 1]!.role === "assistant") c.messages.pop();
    this.streaming = { conversationId: c.id, messageId: null, thinking: "", answer: "", droppedMessages: 0, partialDocuments: false };
    try {
      this.conversation = await api.regenerate(c.id, thinkHarder);
    } catch (e) {
      this.chatError = asEngineError(e);
      this.conversation = await api.conversation(c.id).catch(() => c);
    } finally {
      this.streaming = null;
      this.loadingFraction = null;
      this.readingFraction = null;
    }
  }

  async stop() {
    await api.stop();
  }

  async deleteConversation(id: string) {
    await api.deleteConversation(id);
    if (this.conversation?.id === id) this.conversation = null;
    await this.refresh();
  }

  async switchModel(modelId: string) {
    if (this.conversation) {
      this.conversation = await api.setConversationModel(this.conversation.id, modelId);
    }
    await this.saveSettings({ active_model: modelId });
  }
}

export const app = new App();
