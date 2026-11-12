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

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Settings {
    pub theme: Theme, pub toggle: ToggleSettings, pub placement: Option<Placement>,
    pub panel_width: f64, pub panel_height: f64, pub panel_always_on_top: bool,
    pub autostart: bool, // opt-in, off by default
}
impl Default for Settings {
    fn default() -> Self { Self { theme: Theme::System, toggle: ToggleSettings::default(), placement: None, panel_width: 420.0, panel_height: 620.0, panel_always_on_top: false, autostart: false } }
}
impl Settings {
    pub fn clamped(mut self) -> Self {
        self.toggle = self.toggle.clamped();
        let f = |v: f64, lo: f64, hi: f64, d: f64| if v.is_finite() { v.clamp(lo, hi) } else { d };
        self.panel_width = f(self.panel_width, 320.0, 1400.0, 420.0);
        self.panel_height = f(self.panel_height, 360.0, 1200.0, 620.0);
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
