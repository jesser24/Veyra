import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { Settings, Side } from "./types";

export const api = {
  toggleVisual: () => invoke<Side | null>("toggle_visual"),
  logUi: (msg: string) => invoke<void>("log_ui", { msg }).catch(() => {}),
  onVisual: (f: (s: Side | null) => void) => listen<Side | null>("toggle-visual", (e) => f(e.payload)),
  getSettings: () => invoke<Settings>("get_settings"),
  updateSettings: (s: Settings) => invoke<Settings>("update_settings", { new: s }),
  resetTogglePosition: () => invoke<void>("reset_toggle_position"),
  toggleClicked: () => invoke<void>("toggle_clicked"),
  hidePanel: () => invoke<void>("hide_panel_cmd"),
  quit: () => invoke<void>("quit_app"),
  onSettings: (f: (s: Settings) => void) => listen<Settings>("settings-changed", (e) => f(e.payload)),
  onCollapsed: (f: (c: boolean) => void) => listen<boolean>("panel-collapsed", (e) => f(e.payload)),
};
