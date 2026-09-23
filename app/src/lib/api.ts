// Typed calls into the Rust engine. Inside the app these go through Tauri's in-process IPC.
// In a plain browser (UI tests, design previews) a fake backend from ./mock.ts answers instead.

import type { AppState } from "./bindings/AppState";
import type { Attachment } from "./bindings/Attachment";
import type { BenchmarkResult } from "./bindings/BenchmarkResult";
import type { CatalogRefresh } from "./bindings/CatalogRefresh";
import type { Conversation } from "./bindings/Conversation";
import type { ConversationSummary } from "./bindings/ConversationSummary";
import type { DeviceInfo } from "./bindings/DeviceInfo";
import type { EngineError } from "./bindings/EngineError";
import type { Event as EngineEvent } from "./bindings/Event";
import type { ImportedModel } from "./bindings/ImportedModel";
import type { Recommendations } from "./bindings/Recommendations";
import type { Settings } from "./bindings/Settings";
import type { UseCase } from "./bindings/UseCase";

export type { EngineEvent, EngineError };

export interface Backend {
  invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T>;
  listen(handler: (e: EngineEvent) => void): Promise<() => void>;
  pickFile(options: { documents: boolean }): Promise<string | null>;
  openUrl(url: string): Promise<void>;
}

export const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

async function tauriBackend(): Promise<Backend> {
  const core = await import("@tauri-apps/api/core");
  const event = await import("@tauri-apps/api/event");
  const dialog = await import("@tauri-apps/plugin-dialog");
  const opener = await import("@tauri-apps/plugin-opener");
  return {
    invoke: (cmd, args) => core.invoke(cmd, args),
    listen: (handler) => event.listen<EngineEvent>("engine", (e) => handler(e.payload)),
    async pickFile({ documents }) {
      const picked = await dialog.open({
        multiple: false,
        filters: documents
          ? [{ name: "Documents", extensions: ["txt", "md", "pdf", "docx"] }]
          : [{ name: "Model files", extensions: ["gguf"] }],
      });
      return typeof picked === "string" ? picked : null;
    },
    openUrl: (url) => opener.openUrl(url),
  };
}

let backendPromise: Promise<Backend> | null = null;
export function backend(): Promise<Backend> {
  backendPromise ??= inTauri ? tauriBackend() : import("./mock").then((m) => m.mockBackend());
  return backendPromise;
}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  return (await backend()).invoke<T>(cmd, args);
}

/** Engine errors arrive as tagged objects; anything else (a crash in IPC) becomes `io`. */
export function asEngineError(e: unknown): EngineError {
  if (e && typeof e === "object" && "kind" in e) return e as EngineError;
  return { kind: "io", message: String(e) };
}

export const api = {
  state: () => call<AppState>("get_state"),
  detectDevice: () => call<DeviceInfo>("detect_device"),
  recommend: (useCase: UseCase) => call<Recommendations>("recommend", { useCase }),
  saveSettings: (settings: Settings) => call<Settings>("save_settings", { settings }),
  acceptLicense: (licenseId: string) => call<Settings>("accept_license", { licenseId }),
  refreshCatalog: (force: boolean) => call<CatalogRefresh>("refresh_catalog", { force }),
  download: (modelId: string) => call<void>("download_model", { modelId }),
  cancelDownload: () => call<void>("cancel_download"),
  deleteModel: (modelId: string) => call<void>("delete_model", { modelId }),
  benchmark: (modelId: string) => call<BenchmarkResult>("run_benchmark", { modelId }),
  sendMessage: (conversationId: string | null, text: string, attachments: Attachment[], thinkHarder: boolean) =>
    call<Conversation>("send_message", { conversationId, text, attachments, thinkHarder }),
  regenerate: (conversationId: string, thinkHarder: boolean) =>
    call<Conversation>("regenerate", { conversationId, thinkHarder }),
  stop: () => call<void>("stop_generation"),
  conversations: () => call<ConversationSummary[]>("list_conversations"),
  conversation: (id: string) => call<Conversation>("get_conversation", { id }),
  deleteConversation: (id: string) => call<void>("delete_conversation", { id }),
  deleteAllConversations: () => call<void>("delete_all_conversations"),
  setConversationModel: (id: string, modelId: string) =>
    call<Conversation>("set_conversation_model", { id, modelId }),
  unload: () => call<void>("unload_model"),
  attachFile: (path: string) => call<Attachment>("attach_file", { path }),
  importModel: (path: string) => call<ImportedModel>("import_model", { path }),
};
