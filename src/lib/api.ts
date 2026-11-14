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
  openApp: (section?: string) => invoke<void>("open_app", { section: section ?? null }),
  openChat: (conversation?: string) => invoke<void>("open_chat", { conversation: conversation ?? null }),
  takePendingOpen: () => invoke<string | null>("take_pending_open"),
  onOpenConversation: (f: (id: string | null) => void) => listen<string | null>("open-conversation", (e) => f(e.payload)),
  onAppSection: (f: (s: string) => void) => listen<string>("app-section", (e) => f(e.payload)),
  quit: () => invoke<void>("quit_app"),
  onSettings: (f: (s: Settings) => void) => listen<Settings>("settings-changed", (e) => f(e.payload)),
  onCollapsed: (f: (c: boolean) => void) => listen<boolean>("panel-collapsed", (e) => f(e.payload)),
};
