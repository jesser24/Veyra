import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export type FileState = "available" | "partial" | "downloaded" | "verified";
export interface Hardware { os: string; arch: string; cpu_brand: string; physical_cores: number; logical_cores: number; total_ram_mib: number; available_ram_mib: number; avx2: boolean; avx512: boolean; apple_silicon: boolean }
export interface ModelView { id: string; name: string; file: string; sha256: string | null; size_gb: number; params_b: number; quant: string; license: string; official_gguf: boolean; status: "verified" | "candidate"; min_ram_gb: number; notes: string; state: FileState; bytes_on_disk: number; loaded: boolean; recommended: boolean; fits: boolean }
export interface Perf { prompt_tokens: number | null; generated_tokens: number | null; prompt_tokens_per_s: number | null; generated_tokens_per_s: number | null; wall_ms: number; first_token_ms: number | null; load_ms: number | null }
export interface EngineStatus { loaded: boolean; model: string | null; ctx: number; threads: number; rss_mib: number | null; load_ms: number | null }
export interface Overview { hardware: Hardware; free_disk_bytes: number | null; engine_binary: boolean; models: ModelView[]; recommendation: { id: string; reason: string; warnings: string[] } | null; status: EngineStatus; generating: boolean; downloading: boolean; last_perf: Perf | null; models_dir: string }
export interface DlEvent { id: string; phase: string; done: number; total: number | null; error: string | null }
export interface Conversation { id: string; title: string; archived: boolean; favorite: boolean; deleted_at: number | null; updated: number; created: number; summary: string | null }
export interface Message { id: string; conversation_id: string; role: "user" | "assistant" | "system"; content: string; status: "sent" | "pending" | "error"; created: number }
export interface NetSummary { requests: number; in_flight: number; errors: number; dns_errors: number; tls_errors: number; proxy_errors: number; timeouts: number; bytes_sent: number; bytes_received: number; external_requests: number; local_requests: number; avg_latency_ms: number | null; hosts: string[] }
export interface NetEntry { id: number; purpose: string; method: string; host: string; local: boolean; started_ms: number; duration_ms: number | null; bytes_sent: number; bytes_received: number; status: number | null; error: string | null; in_flight: boolean }

const on = <T,>(ev: string, f: (p: T) => void) => listen<T>(ev, (e) => f(e.payload));
export const ai = {
  overview: () => invoke<Overview>("ai_overview"),
  download: (id: string) => invoke<void>("ai_download", { id }), cancelDownload: () => invoke<void>("ai_download_cancel"),
  verify: (id: string) => invoke<boolean>("ai_verify", { id }), deleteModel: (id: string) => invoke<void>("ai_delete_model", { id }),
  load: (id: string, ctx?: number, threads?: number) => invoke<void>("ai_load", { id, ctx: ctx ?? null, threads: threads ?? null }), unload: () => invoke<void>("ai_unload"),
  status: () => invoke<EngineStatus>("ai_status"), test: () => invoke<{ text: string; perf: Perf; rss_mib: number | null }>("ai_test"), cancel: () => invoke<void>("ai_cancel"),
  onDownload: (f: (e: DlEvent) => void) => on<DlEvent>("ai-download", f), onEngine: (f: (e: EngineStatus) => void) => on<EngineStatus>("ai-engine", f),
};
export const vault = {
  status: () => invoke<{ exists: boolean; unlocked: boolean }>("vault_status"), create: (password: string) => invoke<string>("vault_create", { password }),
  unlock: (password: string) => invoke<void>("vault_unlock", { password }), unlockRecovery: (recovery: string) => invoke<void>("vault_unlock_recovery", { recovery }), lock: () => invoke<void>("vault_lock"),
};
export const chat = {
  list: (view: "active" | "archived" | "trash", query?: string) => invoke<Conversation[]>("conv_list", { view, query: query ?? null }),
  messages: (id: string) => invoke<Message[]>("conv_messages", { id }), create: () => invoke<Conversation>("conv_create"),
  rename: (id: string, title: string) => invoke<void>("conv_rename", { id, title }), archive: (id: string, archived: boolean) => invoke<void>("conv_archive", { id, archived }),
  trash: (id: string) => invoke<void>("conv_trash", { id }), restore: (id: string) => invoke<void>("conv_restore", { id }), remove: (id: string) => invoke<void>("conv_delete_permanently", { id }),
  send: (conversation: string | null, text: string, thinking: boolean) => invoke<{ conversation: Conversation; user_message: Message; assistant_message: Message }>("chat_send", { conversation, text, thinking }),
  onToken: (f: (e: { conversation: string; message: string; delta: string }) => void) => on("chat-token", f),
  onDone: (f: (e: { conversation: string; message: string; perf: Perf; cancelled: boolean; context_messages: number; total_messages: number; summarised: boolean }) => void) => on("chat-done", f),
  onError: (f: (e: { conversation: string; message: string; error: string }) => void) => on("chat-error", f),
};
export const net = { summary: () => invoke<NetSummary>("net_summary"), entries: () => invoke<NetEntry[]>("net_entries"), clear: () => invoke<void>("net_clear") };

export const fmtBytes = (n: number) => n >= 1 << 30 ? `${(n / (1 << 30)).toFixed(2)} GB` : n >= 1 << 20 ? `${(n / (1 << 20)).toFixed(0)} MB` : `${Math.round(n / 1024)} KB`;
export const fmtMs = (n: number | null | undefined) => n == null ? "—" : n >= 1000 ? `${(n / 1000).toFixed(1)} s` : `${n} ms`;
export const fmtRate = (n: number | null | undefined) => n == null ? "—" : `${n.toFixed(1)} tok/s`;
