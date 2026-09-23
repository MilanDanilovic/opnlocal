// A stand-in backend for running the UI in a normal browser: Playwright tests, accessibility
// checks and design previews. Never used inside the app. Pick a device with ?scenario=…
// (laptop | gaming-pc | phone | old-phone | tiny-phone | low-disk | offline | returning).

import type { Backend, EngineEvent } from "./api";
import type { AppState } from "./bindings/AppState";
import type { BenchmarkResult } from "./bindings/BenchmarkResult";
import type { Catalog } from "./bindings/Catalog";
import type { Conversation } from "./bindings/Conversation";
import type { DeviceInfo } from "./bindings/DeviceInfo";
import type { FitLabel } from "./bindings/FitLabel";
import type { ModelOption } from "./bindings/ModelOption";
import type { Placement } from "./bindings/Placement";
import type { Recommendations } from "./bindings/Recommendations";
import type { Settings } from "./bindings/Settings";
import type { UseCase } from "./bindings/UseCase";
import catalogJson from "../../../catalog/catalog.json";

const catalog = catalogJson as unknown as Catalog;
const GB = 1e9;

type Scenario = "laptop" | "gaming-pc" | "phone" | "old-phone" | "tiny-phone" | "low-disk" | "offline" | "returning";

function scenario(): Scenario {
  const s = new URLSearchParams(location.search).get("scenario");
  return (s as Scenario) ?? "laptop";
}

function device(s: Scenario): DeviceInfo {
  const base: DeviceInfo = {
    platform: "windows",
    os_version: "Windows 11 Pro",
    device_name: null,
    cpu: { name: "Intel Core i7-1360P", cores: 12, threads: 16, arch: "x86_64", features: ["avx", "avx2", "fma", "f16c"] },
    memory: { total: 16 * GB, available: 9 * GB, app_limit: null },
    gpus: [{ name: "Intel Iris Xe Graphics", backend: "Vulkan", kind: "integrated", memory_total: 8 * GB, memory_free: 7 * GB, device_index: 0 }],
    storage: { free: 180 * GB, total: 512 * GB },
    issues: [],
  };
  switch (s) {
    case "gaming-pc":
      return {
        ...base,
        cpu: { ...base.cpu, name: "AMD Ryzen 7 7700X 8-Core Processor", cores: 8, threads: 16 },
        memory: { total: 32 * GB, available: 20 * GB, app_limit: null },
        gpus: [{ name: "AMD Radeon RX 7800 XT", backend: "Vulkan", kind: "discrete", memory_total: 16 * GB, memory_free: 15.4 * GB, device_index: 0 }],
      };
    case "phone":
      return {
        ...base,
        platform: "android",
        os_version: "Android 15",
        device_name: "Google Pixel 8",
        cpu: { name: "Google Tensor G3", cores: 9, threads: 9, arch: "aarch64", features: ["neon", "dotprod", "fp16", "i8mm"] },
        memory: { total: 8 * GB, available: 3.5 * GB, app_limit: null },
        gpus: [],
        storage: { free: 40 * GB, total: 128 * GB },
      };
    case "tiny-phone":
      return { ...device("phone"), device_name: "Budget phone", memory: { total: 1.5 * GB, available: 0.5 * GB, app_limit: null } };
    case "old-phone":
      return {
        ...device("phone"),
        device_name: "Samsung Galaxy J7",
        cpu: { name: "Exynos 7870", cores: 8, threads: 8, arch: "aarch64", features: ["neon"] },
        issues: [{ kind: "cpu_too_old", missing: ["dotprod", "fp16"] }],
      };
    case "low-disk":
      return { ...base, storage: { free: 2.1 * GB, total: 256 * GB } };
    default:
      return base;
  }
}

function settings(s: Scenario): Settings {
  const returning = s === "returning";
  return {
    onboarding_done: returning,
    use_case: returning ? "everyday" : null,
    active_model: returning ? "qwen3.5-4b" : null,
    auto_refresh_catalog: true,
    last_catalog_check: null,
    accepted_licenses: {},
    gpu_disabled: false,
    gpu_crash: null,
    per_model: {},
  };
}

/** Rough stand-in for the engine's fit math (the real one lives in Rust and is unit-tested). */
function fitFor(d: DeviceInfo, weights: number): { label: FitLabel; placement: Placement; need: number } {
  const mobile = d.platform === "android" || d.platform === "ios";
  const need = weights * 1.15 + 0.4 * GB;
  const [c, l] = mobile ? [0.35, 0.5] : [0.5, 0.7];
  const gpu = d.gpus.find((g) => g.kind === "discrete");
  if (gpu && need <= gpu.memory_free * 0.9) return { label: "comfortable", placement: "gpu", need };
  const spill = gpu ? need - gpu.memory_free * 0.9 : need;
  const placement: Placement = gpu ? "mixed" : "cpu";
  if (spill <= d.memory.total * c) return { label: "comfortable", placement, need };
  if (spill <= d.memory.total * l) return { label: "tight", placement, need };
  return { label: "too_big", placement, need };
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

export function mockBackend(): Backend {
  const s = scenario();
  const dev = device(s);
  const listeners: ((e: EngineEvent) => void)[] = [];
  const emit = (e: EngineEvent) => listeners.forEach((l) => l(e));
  const installed = new Set<string>(s === "returning" ? ["qwen3.5-4b"] : []);
  const conversations = new Map<string, Conversation>();
  let cancelled = false;
  let stopFlag = false;
  let st: Settings = settings(s);
  const benchmarks: Record<string, BenchmarkResult> = {};

  if (s === "returning") {
    conversations.set("c1", {
      id: "c1",
      title: "Plan a week of simple dinners",
      model_id: "qwen3.5-4b",
      use_case: "everyday",
      think_harder: false,
      created_at: Date.now() / 1000 - 3600,
      updated_at: Date.now() / 1000 - 3600,
      messages: [
        { id: "m1", role: "user", content: "Plan a week of simple dinners for two.", thinking: null, attachments: [], created_at: 0, stats: null, stopped: false },
        {
          id: "m2",
          role: "assistant",
          content: "Here's an easy week:\n\n| Day | Dinner |\n|---|---|\n| Mon | Tomato pasta with spinach |\n| Tue | Chicken stir-fry |\n| Wed | Lentil soup and bread |\n\n**Tip:** cook double on Monday and keep half for Thursday.",
          thinking: null,
          attachments: [],
          created_at: 0,
          stats: { tokens: 90, seconds: 6.1, words: 48 },
          stopped: false,
        },
      ],
    });
    benchmarks["qwen3.5-4b"] = bench("qwen3.5-4b", dev);
  }

  const state = (): AppState => ({
    app_version: "0.1.0",
    platform: dev.platform,
    settings: st,
    catalog_version: catalog.version,
    models: catalog.models,
    imported: [],
    installed: [...installed],
    partial_downloads: [],
    active_download: null,
    benchmarks,
    conversations: [...conversations.values()]
      .sort((a, b) => b.updated_at - a.updated_at)
      .map((c) => ({ id: c.id, title: c.title, model_id: c.model_id, updated_at: c.updated_at })),
    storage_path: "C:\\Users\\you\\AppData\\Roaming\\io.github.milandanilovic.opnlocal",
    loaded: null,
  });

  function recommend(useCase: UseCase): Recommendations {
    const all: ModelOption[] = catalog.models.map((m) => {
      const f = fitFor(dev, m.memory.weights_bytes);
      const need = m.file.size + 0.5 * GB;
      return {
        model_id: m.id,
        fit: { label: f.label, placement: f.placement, context: 16384, need_bytes: f.need, short_by_bytes: f.label === "too_big" ? f.need - dev.memory.total * 0.7 : 0, close_other_apps: false },
        storage: { ok: need <= dev.storage.free, need_bytes: need, short_by_bytes: Math.max(0, need - dev.storage.free) },
        score: m.scores[useCase],
        installed: installed.has(m.id),
      };
    });
    const order: Record<FitLabel, number> = { comfortable: 0, tight: 1, too_big: 2 };
    all.sort((a, b) => order[a.fit.label] - order[b.fit.label] || b.score - a.score);
    if (dev.issues.some((i) => i.kind === "cpu_too_old")) return { use_case: useCase, picks: [], all, unsupported: { kind: "cpu_too_old" } };
    const ok = all.filter((o) => o.score > 0);
    const comfortable = ok.filter((o) => o.fit.label === "comfortable");
    const tight = ok.filter((o) => o.fit.label === "tight");
    const rec = comfortable[0] ?? tight[0];
    if (!rec) return { use_case: useCase, picks: [], all, unsupported: { kind: "not_enough_memory", smallest_need_bytes: Math.min(...ok.map((o) => o.fit.need_bytes)) } };
    const w = (id: string) => catalog.models.find((m) => m.id === id)!.memory.weights_bytes;
    const picks = [{ slot: "recommended" as const, option: rec }];
    const lighter = comfortable.find((o) => w(o.model_id) * 4 <= w(rec.model_id) * 3);
    if (lighter) picks.push({ slot: "lighter" as never, option: lighter });
    const stronger = rec.fit.label === "comfortable" ? tight.find((o) => o.score > rec.score) : undefined;
    if (stronger) picks.push({ slot: "stronger" as never, option: stronger });
    return { use_case: useCase, picks, all, unsupported: null };
  }

  const commands: Record<string, (a: Record<string, any>) => Promise<unknown>> = {
    get_state: async () => state(),
    detect_device: async () => {
      await sleep(900);
      return dev;
    },
    recommend: async (a) => recommend(a.useCase),
    save_settings: async (a) => (st = a.settings),
    accept_license: async (a) => (st = { ...st, accepted_licenses: { ...st.accepted_licenses, [a.licenseId]: Date.now() / 1000 } }),
    refresh_catalog: async () => {
      await sleep(400);
      if (s === "offline") throw { kind: "catalog_unavailable", message: "offline" };
      return { checked: true, updated: false, version: catalog.version };
    },
    download_model: async (a) => {
      const m = catalog.models.find((x) => x.id === a.modelId)!;
      cancelled = false;
      const total = m.file.size;
      for (let i = 1; i <= 40; i++) {
        await sleep(80);
        if (cancelled) {
          emit({ type: "download_finished", model_id: m.id, error: { kind: "cancelled" } });
          throw { kind: "download", error: { kind: "cancelled" } };
        }
        if (s === "offline" && i === 14) {
          emit({ type: "download_finished", model_id: m.id, error: { kind: "offline" } });
          throw { kind: "download", error: { kind: "offline" } };
        }
        emit({ type: "download_progress", model_id: m.id, progress: { stage: "downloading", done: (total * i) / 40, total, bytes_per_second: i > 3 ? 42_000_000 : null } });
      }
      installed.add(m.id);
      emit({ type: "download_finished", model_id: m.id, error: null });
    },
    cancel_download: async () => {
      cancelled = true;
    },
    delete_model: async (a) => {
      installed.delete(a.modelId);
    },
    run_benchmark: async (a) => {
      for (const stage of ["loading", "reading", "writing"] as const) {
        emit({ type: "benchmark_stage", model_id: a.modelId, stage });
        await sleep(700);
      }
      benchmarks[a.modelId] = bench(a.modelId, dev);
      return benchmarks[a.modelId];
    },
    send_message: async (a) => {
      let c = a.conversationId ? conversations.get(a.conversationId)! : undefined;
      if (!c) {
        c = { id: `c${conversations.size + 1}`, title: a.text.slice(0, 60) || "New chat", model_id: st.active_model ?? "qwen3.5-4b", use_case: st.use_case ?? "everyday", think_harder: a.thinkHarder, created_at: Date.now() / 1000, updated_at: Date.now() / 1000, messages: [] };
        conversations.set(c.id, c);
      }
      c.messages.push({ id: `u${Date.now()}`, role: "user", content: a.text, thinking: null, attachments: a.attachments, created_at: Date.now() / 1000, stats: null, stopped: false });
      return reply(c, a.thinkHarder);
    },
    regenerate: async (a) => {
      const c = conversations.get(a.conversationId)!;
      while (c.messages.at(-1)?.role === "assistant") c.messages.pop();
      return reply(c, a.thinkHarder);
    },
    stop_generation: async () => {
      stopFlag = true;
    },
    list_conversations: async () => state().conversations,
    get_conversation: async (a) => structuredClone(conversations.get(a.id)),
    delete_conversation: async (a) => conversations.delete(a.id),
    delete_all_conversations: async () => conversations.clear(),
    set_conversation_model: async (a) => {
      const c = conversations.get(a.id)!;
      c.model_id = a.modelId;
      return structuredClone(c);
    },
    unload_model: async () => {},
    attach_file: async (a) => ({ name: String(a.path).split(/[\\/]/).pop(), text: "Quarterly notes. Revenue grew 4%. Deadline for the report is 14 October." }),
    import_model: async () => {
      throw { kind: "bad_model_file", message: "this is not a GGUF model file" };
    },
  };

  async function reply(c: Conversation, thinkHarder: boolean): Promise<Conversation> {
    stopFlag = false;
    const messageId = `a${Date.now()}`;
    for (let i = 0; i <= 10; i++) {
      emit({ type: "model_loading", model_id: c.model_id, fraction: i / 10 });
      await sleep(30);
    }
    emit({ type: "chat_started", conversation_id: c.id, message_id: messageId, dropped_messages: 0 });
    const thinking = thinkHarder ? "The user wants a clear answer. I'll structure it as steps and keep it short." : "";
    for (const w of thinking.split(" ")) {
      if (!w) continue;
      emit({ type: "chat_delta", conversation_id: c.id, message_id: messageId, thinking: w + " ", answer: null });
      await sleep(90);
    }
    const answer = "Sure! Here's a simple way to do it:\n\n1. **Start small.** Pick one thing to change this week.\n2. **Write it down.** A short note helps you remember.\n3. **Check in on Friday.** See what worked.\n\n```python\nfor day in ['Mon', 'Tue', 'Wed']:\n    print(f'{day}: done')\n```\n\nWant me to make a checklist for you?";
    let out = "";
    let stopped = false;
    for (const w of answer.split(/(?<= )/)) {
      if (stopFlag) {
        stopped = true;
        break;
      }
      out += w;
      emit({ type: "chat_delta", conversation_id: c.id, message_id: messageId, thinking: null, answer: w });
      await sleep(35);
    }
    c.messages.push({ id: messageId, role: "assistant", content: out.trim(), thinking: thinking || null, attachments: [], created_at: Date.now() / 1000, stats: { tokens: 80, seconds: 3.2, words: out.split(/\s+/).length }, stopped });
    c.updated_at = Date.now() / 1000;
    return structuredClone(c);
  }

  return {
    async invoke<T>(cmd: string, args: Record<string, unknown> = {}) {
      const f = commands[cmd];
      if (!f) throw { kind: "io", message: `mock: unknown command ${cmd}` };
      // Real IPC serializes arguments to JSON; do the same (this also unwraps Svelte state proxies).
      return (await f(JSON.parse(JSON.stringify(args)))) as T;
    },
    async listen(handler) {
      listeners.push(handler);
      return () => listeners.splice(listeners.indexOf(handler), 1);
    },
    async pickFile({ documents }) {
      return documents ? "C:\\Users\\you\\Documents\\notes.txt" : "C:\\Users\\you\\Downloads\\model.gguf";
    },
    async openUrl(url) {
      window.open(url, "_blank", "noopener");
    },
  };
}

function bench(modelId: string, d: DeviceInfo): BenchmarkResult {
  const gpu = d.gpus.find((g) => g.kind === "discrete");
  const wps = gpu ? 38 : d.platform === "android" ? 7.5 : 11;
  return {
    model_id: modelId,
    app_version: "0.1.0",
    measured_at: Date.now() / 1000,
    device: gpu?.name ?? d.cpu.name,
    placement: gpu ? "gpu" : "cpu",
    gpu_layers: gpu ? 33 : 0,
    context: 16384,
    load_ms: 2100,
    prompt_tokens: 402,
    prompt_tokens_per_second: gpu ? 1900 : 160,
    first_token_ms: gpu ? 420 : 2800,
    generated_tokens: 160,
    generation_tokens_per_second: wps * 1.35,
    words: 118,
    words_per_second: wps,
    verdict: wps >= 7.9 ? "faster_than_reading" : "about_reading_speed",
    ram_in_use_bytes: 3.1 * GB,
    gpu_memory_used_bytes: gpu ? 3.4 * GB : null,
    hit_time_limit: false,
  };
}
