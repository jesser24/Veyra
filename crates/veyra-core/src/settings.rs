//! User settings. Unknown fields are tolerated (forward compatible); every value is clamped on load.
use crate::geometry::Placement;
use serde::{Deserialize, Serialize};
use std::{fs, io, path::Path};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Theme { System, Light, Dark }

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Shape { Tab, #[serde(alias = "circle")] Pill, #[serde(alias = "squircle")] Rounded }

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SecondClick { Hide, Collapse }

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct ToggleSettings {
    pub thickness: f64, pub length: f64, pub opacity: f64, pub margin: f64, pub snap_threshold: f64,
    pub shape: Shape, pub accent: String, pub shadow: bool, pub reflection: bool, pub animations: bool,
    pub always_on_top: bool, pub second_click: SecondClick,
}
impl Default for ToggleSettings {
    fn default() -> Self { Self { thickness: 10.0, length: 72.0, opacity: 0.75, margin: 12.0, snap_threshold: 40.0, shape: Shape::Tab,
        accent: "#6f7de0".into(), shadow: true, reflection: true, animations: true, always_on_top: true, second_click: SecondClick::Hide } }
}
impl ToggleSettings {
    pub fn clamped(mut self) -> Self {
        let f = |v: f64, lo: f64, hi: f64, d: f64| if v.is_finite() { v.clamp(lo, hi) } else { d };
        self.thickness = f(self.thickness, 6.0, 20.0, 10.0);
        self.length = f(self.length, 40.0, 200.0, 72.0);
        self.opacity = f(self.opacity, 0.15, 1.0, 0.75); // never fully invisible: the hit area must stay usable
        self.margin = f(self.margin, 0.0, 96.0, 12.0);
        self.snap_threshold = f(self.snap_threshold, 0.0, 120.0, 40.0);
        let hex = self.accent.len() == 7 && self.accent.starts_with('#') && self.accent[1..].chars().all(|c| c.is_ascii_hexdigit());
        if !hex { self.accent = "#6f7de0".into(); }
        self
    }
}

/// Local-AI preferences, shared by the chat window and the full application (one source of truth).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct AiSettings {
    /// Catalog id of the model to use; set when the user loads a model. `None` = nothing selected yet.
    pub model_id: Option<String>,
    /// Load the selected (downloaded and SHA-256-verified) model when Veyra starts.
    pub auto_load: bool,
    /// Unload the model after this many idle minutes. `None` (default) = never.
    pub idle_unload_minutes: Option<u32>,
    /// Reasoning: "off" (fast, default), "on" (the model thinks before answering; slower).
    pub thinking: String,
    /// Context size in tokens; `None` = automatic (4096 on CPU-only machines, 8192 on Apple Silicon).
    pub ctx: Option<u32>,
    /// Generation threads; `None` = automatic (physical cores).
    pub threads: Option<u32>,
    pub batch: u32,
    /// "auto" | "on" | "off"
    pub flash_attention: String,
    /// Store the KV cache as 8-bit (about half the memory, tiny quality cost).
    pub kv_cache_q8: bool,
    /// Maximum tokens per reply.
    pub max_reply_tokens: u32,
    /// Show the engine's performance figures in the AI section of the application (never in the chat).
    pub show_performance: bool,
}
impl Default for AiSettings {
    fn default() -> Self { Self { model_id: None, auto_load: true, idle_unload_minutes: None, thinking: "off".into(), ctx: None, threads: None, batch: 512, flash_attention: "auto".into(), kv_cache_q8: false, max_reply_tokens: 1024, show_performance: true } }
}
impl AiSettings {
    pub fn clamped(mut self) -> Self {
        self.batch = self.batch.clamp(64, 4096); self.max_reply_tokens = self.max_reply_tokens.clamp(16, 8192);
        self.ctx = self.ctx.map(|c| c.clamp(1024, 32768)); self.threads = self.threads.map(|t| t.clamp(1, 64)); self.idle_unload_minutes = self.idle_unload_minutes.map(|m| m.clamp(1, 24 * 60));
        if !matches!(self.thinking.as_str(), "off" | "on") { self.thinking = "off".into(); } if !matches!(self.flash_attention.as_str(), "auto" | "on" | "off") { self.flash_attention = "auto".into(); } self
    }
}

/// Look-and-feel shared by the chat window and the application. Every field is applied by the front-end (CSS variables / behaviour); none is decorative.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct UiSettings {
    /// Text scale, 0.85–1.3.
    pub text_size: f64,
    /// "comfortable" | "compact".
    pub density: String,
    /// Opacity of the window surfaces, 0.88–1.0 (never see-through enough to hurt legibility).
    pub surface_opacity: f64,
    /// Accent colour for both windows; `None` = same as the toggle's.
    pub accent: Option<String>,
    /// Enter sends (Shift+Enter = new line) when true; Ctrl/Cmd+Enter sends when false.
    pub enter_sends: bool,
}
impl Default for UiSettings { fn default() -> Self { Self { text_size: 1.0, density: "comfortable".into(), surface_opacity: 0.97, accent: None, enter_sends: true } } }
impl UiSettings {
    pub fn clamped(mut self) -> Self {
        self.text_size = if self.text_size.is_finite() { self.text_size.clamp(0.85, 1.3) } else { 1.0 };
        self.surface_opacity = if self.surface_opacity.is_finite() { self.surface_opacity.clamp(0.88, 1.0) } else { 0.97 };
        if !matches!(self.density.as_str(), "comfortable" | "compact") { self.density = "comfortable".into(); }
        if let Some(a) = &self.accent { let ok = a.len() == 7 && a.starts_with('#') && a[1..].chars().all(|c| c.is_ascii_hexdigit()); if !ok { self.accent = None; } } self
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Settings {
    pub theme: Theme, pub toggle: ToggleSettings, pub placement: Option<Placement>,
    pub panel_width: f64, pub panel_height: f64, pub panel_always_on_top: bool,
    pub autostart: bool, // opt-in, off by default
    pub ai: AiSettings,
    pub ui: UiSettings,
    /// Set once the first-run installation step has been completed (see docs/SECURITY.md: it is NOT a security boundary).
    pub setup_done: bool,
}
impl Default for Settings {
    fn default() -> Self { Self { theme: Theme::System, toggle: ToggleSettings::default(), placement: None, panel_width: 420.0, panel_height: 620.0, panel_always_on_top: false, autostart: false, ai: AiSettings::default(), ui: UiSettings::default(), setup_done: false } }
}
impl Settings {
    pub fn clamped(mut self) -> Self {
        self.toggle = self.toggle.clamped();
        let f = |v: f64, lo: f64, hi: f64, d: f64| if v.is_finite() { v.clamp(lo, hi) } else { d };
        self.panel_width = f(self.panel_width, 320.0, 1400.0, 420.0);
        self.panel_height = f(self.panel_height, 360.0, 1200.0, 620.0);
        self.ai = self.ai.clamped(); self.ui = self.ui.clamped();
        self
    }
    /// Missing or corrupt file -> defaults. A corrupt file is renamed to `<name>.corrupt` instead of being silently overwritten.
    pub fn load(path: &Path) -> Settings {
        match fs::read_to_string(path) {
            Ok(s) => match serde_json::from_str::<Settings>(&s) {
                Ok(v) => v.clamped(),
                Err(_) => { let _ = fs::rename(path, path.with_extension("json.corrupt")); Settings::default() }
            },
            Err(_) => Settings::default(),
        }
    }
    /// Atomic write (temp file + rename) so a crash never leaves a half-written settings file.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(d) = path.parent() { fs::create_dir_all(d)?; }
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, serde_json::to_vec_pretty(&self.clone().clamped()).map_err(io::Error::other)?)?;
        fs::rename(tmp, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn dir() -> std::path::PathBuf { let d = std::env::temp_dir().join(format!("veyra-test-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos())); fs::create_dir_all(&d).unwrap(); d }
    #[test] fn defaults_keep_autostart_off_and_toggle_visible() {
        let s = Settings::default(); assert!(!s.autostart); assert!(s.toggle.opacity >= 0.15);
    }
    #[test] fn values_are_clamped_and_garbage_is_replaced() {
        let t = ToggleSettings { thickness: 9999.0, opacity: 0.0, margin: -4.0, accent: "red".into(), ..Default::default() }.clamped();
        assert_eq!((t.thickness, t.opacity, t.margin), (20.0, 0.15, 0.0)); assert_eq!(t.accent, "#6f7de0");
    }
    #[test] fn ui_settings_are_clamped() { let u = UiSettings { text_size: 9.0, surface_opacity: 0.1, density: "x".into(), accent: Some("blue".into()), enter_sends: true }.clamped(); assert_eq!((u.text_size, u.surface_opacity), (1.3, 0.88)); assert_eq!(u.density, "comfortable"); assert!(u.accent.is_none()); }
    #[test] fn roundtrip_and_old_files_without_new_keys() {
        let d = dir(); let p = d.join("settings.json");
        let mut s = Settings::default(); s.toggle.length = 90.0; s.save(&p).unwrap();
        assert_eq!(Settings::load(&p).toggle.length, 90.0);
        fs::write(&p, r#"{"theme":"dark"}"#).unwrap();
        let o = Settings::load(&p); assert_eq!(o.theme, Theme::Dark); assert_eq!(o.toggle, ToggleSettings::default());
    }
    #[test] fn default_opacity_is_75_percent_and_old_circle_shape_still_loads() {
        assert_eq!(ToggleSettings::default().opacity, 0.75);
        let o: Settings = serde_json::from_str(r#"{"toggle":{"shape":"circle","size":52}}"#).unwrap();
        assert_eq!(o.toggle.shape, Shape::Pill);
    }
    #[test] fn corrupt_file_is_set_aside_not_destroyed() {
        let d = dir(); let p = d.join("settings.json"); fs::write(&p, "{ not json").unwrap();
        assert_eq!(Settings::load(&p), Settings::default());
        assert!(d.join("settings.json.corrupt").exists());
    }
}

#[cfg(test)]
mod ai_tests {
    use super::*;
    #[test] fn old_settings_files_load_with_ai_defaults_and_ai_values_are_clamped() {
        let s: Settings = serde_json::from_str(r#"{"theme":"dark"}"#).unwrap(); assert!(s.ai.auto_load && s.ai.idle_unload_minutes.is_none() && s.ai.thinking == "off");
        let mut s = Settings::default(); s.ai.ctx = Some(10); s.ai.threads = Some(0); s.ai.thinking = "weird".into(); s.ai.idle_unload_minutes = Some(0); s.ai.max_reply_tokens = 99999;
        let c = s.clamped(); assert_eq!((c.ai.ctx, c.ai.threads, c.ai.idle_unload_minutes, c.ai.max_reply_tokens), (Some(1024), Some(1), Some(1), 8192)); assert_eq!(c.ai.thinking, "off");
    }
}
