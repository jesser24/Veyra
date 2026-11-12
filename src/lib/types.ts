/** Mirrors `veyra_core::settings` (JSON produced by the Rust side). */
export type Theme = "system" | "light" | "dark";
export type Shape = "circle" | "squircle" | "pill";
export type SecondClick = "hide" | "collapse";
export interface ToggleSettings {
  size: number; opacity: number; margin: number; snap_threshold: number; shape: Shape; accent: string;
  shadow: boolean; reflection: boolean; animations: boolean; always_on_top: boolean; second_click: SecondClick;
}
export interface Settings {
  theme: Theme; toggle: ToggleSettings; placement: unknown; panel_width: number; panel_height: number;
  panel_always_on_top: boolean; autostart: boolean;
}
