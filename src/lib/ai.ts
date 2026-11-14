import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export type FileState = "available" | "partial" | "downloaded" | "verified";
export interface Hardware { os: string; arch: string; cpu_brand: string; physical_cores: number; logical_cores: number; total_ram_mib: number; available_ram_mib: number; avx2: boolean; avx512: boolean; apple_silicon: boolean }
export interface ModelView { id: string; name: string; file: string; sha256: string | null; size_gb: number; params_b: number; quant: string; license: string; official_gguf: boolean; status: "verified" | "candidate"; min_ram_gb: number; notes: string; state: FileState; bytes_on_disk: number; loaded: boolean; recommended: boolean; fits: boolean }
export interface Perf { prompt_tokens: number | null; generated_tokens: number | null; prompt_tokens_per_s: number | null; generated_tokens_per_s: number | null; wall_ms: number; first_token_ms: number | null; load_ms: number | null }
export interface EngineStatus { loaded: boolean; model: string | null; ctx: number; threads: number; rss_mib: number | null; load_ms: number | null; in_process: boolean; llama: string }
export interface EngineState { status: EngineStatus; loading: boolean; error: string | null; selected: string | null; generating: boolean; downloading: boolean; stub: boolean }
export interface Overview { hardware: Hardware; free_disk_bytes: number | null; models: ModelView[]; recommendation: { id: string; reason: string; warnings: string[] } | null; engine: EngineState; last_perf: Perf | null; models_dir: string }
export interface DlEvent { id: string; phase: string; done: number; total: number | null; error: string | null }
export interface Conversation { id: string; title: string; archived: boolean; favorite: boolean; deleted_at: number | null; updated: number; created: number; summary: string | null }
export interface Message { id: string; conversation_id: string; role: "user" | "assistant" | "system"; content: string; status: "sent" | "pending" | "error"; created: number }
export type AttachKind = "image" | "pdf" | "word" | "sheet" | "slides" | "csv" | "text" | "code" | "archive" | "unsupported";
export interface Analysis { token: string; name: string; size: number; kind: AttachKind; mime: string; summary: string; notes: string[]; chars: number; truncated: boolean; image: [number, number] | null; entries: Array<{ path: string; size: number; read: boolean }>; readable_by_model: boolean }
export interface AttachmentRow { id: string; message_id: string | null; name: string; mime: string; size: number; kind: AttachKind; summary: string; notes: string; width: number | null; height: number | null }
export interface SecurityStatus { ready: boolean; error: string | null; key_protection: string; key_kind: string; data_dir: string; setup_done: boolean }
export interface BackupInfo { name: string; bytes: number; modified: number }
export interface NetSummary { requests: number; in_flight: number; errors: number; dns_errors: number; tls_errors: number; proxy_errors: number; timeouts: number; bytes_sent: number; bytes_received: number; external_requests: number; local_requests: number; avg_latency_ms: number | null; hosts: string[] }
export interface NetEntry { id: number; purpose: string; method: string; host: string; local: boolean; started_ms: number; duration_ms: number | null; bytes_sent: number; bytes_received: number; status: number | null; error: string | null; in_flight: boolean }

const on = <T,>(ev: string, f: (p: T) => void) => listen<T>(ev, (e) => f(e.payload));
export const ai = {
  overview: () => invoke<Overview>("ai_overview"), state: () => invoke<EngineState>("ai_state"),
  download: (id: string) => invoke<void>("ai_download", { id }), cancelDownload: () => invoke<void>("ai_download_cancel"),
  verify: (id: string) => invoke<boolean>("ai_verify", { id }), deleteModel: (id: string) => invoke<void>("ai_delete_model", { id }),
  load: (id: string) => invoke<void>("ai_load", { id }), unload: () => invoke<void>("ai_unload"),
  test: () => invoke<{ text: string; perf: Perf; rss_mib: number | null }>("ai_test"), cancel: () => invoke<void>("ai_cancel"),
  onDownload: (f: (e: DlEvent) => void) => on<DlEvent>("ai-download", f), onState: (f: (e: EngineState) => void) => on<EngineState>("ai-state", f),
};
export const security = {
  status: () => invoke<SecurityStatus>("security_status"), exportKey: (passphrase: string) => invoke<string>("key_export", { passphrase }),
  backupCreate: () => invoke<string>("backup_create"), backupList: () => invoke<BackupInfo[]>("backup_list"), setup: (key: string) => invoke<void>("setup_complete", { key }),
  onReady: (f: () => void) => on<null>("store-ready", () => f()),
};
export const chat = {
  list: (view: "active" | "archived" | "trash", query?: string) => invoke<Conversation[]>("conv_list", { view, query: query ?? null }),
  messages: (id: string) => invoke<Message[]>("conv_messages", { id }), attachments: (id: string) => invoke<AttachmentRow[]>("conv_attachments", { id }),
  create: () => invoke<Conversation>("conv_create"),
  rename: (id: string, title: string) => invoke<void>("conv_rename", { id, title }), archive: (id: string, archived: boolean) => invoke<void>("conv_archive", { id, archived }),
  trash: (id: string) => invoke<void>("conv_trash", { id }), restore: (id: string) => invoke<void>("conv_restore", { id }), remove: (id: string) => invoke<void>("conv_delete_permanently", { id }),
  send: (conversation: string | null, text: string, attachments: string[]) => invoke<{ conversation: Conversation; user_message: Message; assistant_message: Message }>("chat_send", { conversation, text, attachments }),
  onToken: (f: (e: { conversation: string; message: string; delta: string }) => void) => on("chat-token", f),
  onDone: (f: (e: { conversation: string; message: string; perf: Perf; cancelled: boolean; summarised: boolean }) => void) => on("chat-done", f),
  onError: (f: (e: { conversation: string; message: string; error: string }) => void) => on("chat-error", f),
  onListChanged: (f: () => void) => on<null>("conversations-changed", () => f()),
};
export const files = {
  /** The file travels as the raw request body (no base64), its name in a header. Analysis happens in Rust, in memory. */
  stage: async (file: File): Promise<Analysis> => invoke<Analysis>("attach_stage", new Uint8Array(await file.arrayBuffer()), { headers: { "x-name": encodeURIComponent(file.name || "file") } }),
  discard: (token: string) => invoke<void>("attach_discard", { token }),
  stored: async (id: string): Promise<ArrayBuffer> => invoke<ArrayBuffer>("attachment_data", { id }),
  staged: async (token: string): Promise<ArrayBuffer> => invoke<ArrayBuffer>("staged_data", { token }),
};
export const net = { summary: () => invoke<NetSummary>("net_summary"), entries: () => invoke<NetEntry[]>("net_entries"), clear: () => invoke<void>("net_clear") };

export const fmtBytes = (n: number) => n >= 1 << 30 ? `${(n / (1 << 30)).toFixed(2)} GB` : n >= 1 << 20 ? `${(n / (1 << 20)).toFixed(1)} MB` : `${Math.max(1, Math.round(n / 1024))} KB`;
export const fmtMs = (n: number | null | undefined) => n == null ? "—" : n >= 1000 ? `${(n / 1000).toFixed(1)} s` : `${n} ms`;
export const fmtRate = (n: number | null | undefined) => n == null ? "—" : `${n.toFixed(1)} tok/s`;
