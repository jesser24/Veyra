/** Mirrors `veyra_core::settings` (JSON produced by the Rust side). */
export type Theme = "system" | "light" | "dark";
export type Shape = "tab" | "pill" | "rounded";
export type Side = "left" | "right" | "top" | "bottom";
export type SecondClick = "hide" | "collapse";
export interface ToggleSettings {
  thickness: number; length: number; opacity: number; margin: number; snap_threshold: number; shape: Shape; accent: string;
  shadow: boolean; reflection: boolean; animations: boolean; always_on_top: boolean; second_click: SecondClick;
}
export interface AiSettings {
  model_id: string | null; auto_load: boolean; idle_unload_minutes: number | null; thinking: "off" | "on"; ctx: number | null; threads: number | null;
  batch: number; flash_attention: "auto" | "on" | "off"; kv_cache_q8: boolean; max_reply_tokens: number; show_performance: boolean;
}
export interface UiSettings { text_size: number; density: "comfortable" | "compact"; surface_opacity: number; accent: string | null; enter_sends: boolean }
export interface Settings {
  theme: Theme; toggle: ToggleSettings; placement: unknown; panel_width: number; panel_height: number;
  panel_always_on_top: boolean; autostart: boolean; ai: AiSettings; ui: UiSettings; setup_done: boolean;
}
