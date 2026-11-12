//! Veyra native shell (Tauri 2): the floating toggle, the panel window and settings persistence.
//! Pure logic (geometry, settings) lives in `veyra-core`. This file makes no network connection of any kind.
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::SeqCst};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, Position, Size, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent};
use veyra_core::geometry::{self, MonitorInfo, PRect};
use veyra_core::settings::{SecondClick, Settings, ToggleSettings};

const TOGGLE: &str = "toggle";
const PANEL: &str = "panel";
/// Transparent halo around the visible toggle (room for the shadow), in dip.
const HALO: f64 = 12.0;
const COMPACT_H: f64 = 64.0;

struct AppState {
    path: PathBuf,
    settings: RwLock<Settings>,
    move_gen: AtomicU64,
    ignore_moves_until: AtomicU64,
    panel_collapsed: AtomicBool,
    topology: Mutex<String>,
}
type S = Arc<AppState>;

fn now_ms() -> u64 { SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0) }
fn state(app: &AppHandle) -> S { app.state::<S>().inner().clone() }
fn settings(app: &AppHandle) -> Settings { state(app).settings.read().unwrap_or_else(|e| e.into_inner()).clone() }

fn monitors(app: &AppHandle) -> Vec<MonitorInfo> {
    app.available_monitors().unwrap_or_default().iter().map(|m| {
        let wa = m.work_area();
        let id = format!("{}|{}x{}", m.name().cloned().unwrap_or_default(), m.size().width, m.size().height);
        MonitorInfo { id, x: wa.position.x, y: wa.position.y, w: wa.size.width as i32, h: wa.size.height as i32, scale: m.scale_factor() }
    }).collect()
}
fn topology_signature(ms: &[MonitorInfo]) -> String { ms.iter().map(|m| format!("{}:{}:{}:{}:{}:{}", m.id, m.x, m.y, m.w, m.h, m.scale)).collect::<Vec<_>>().join(";") }

fn set_rect(w: &WebviewWindow, r: PRect) {
    let _ = w.set_size(Size::Physical(PhysicalSize::new(r.w.max(1) as u32, r.h.max(1) as u32)));
    let _ = w.set_position(Position::Physical(PhysicalPosition::new(r.x, r.y)));
}
fn window_dip(t: &ToggleSettings) -> f64 { t.size + 2.0 * HALO }
fn core_margin(t: &ToggleSettings) -> f64 { t.margin - HALO }

fn toggle_rect_now(app: &AppHandle) -> Option<(PRect, MonitorInfo)> {
    let s = settings(app);
    let ms = monitors(app);
    let saved = s.placement.clone().unwrap_or_default();
    let (p, m) = geometry::resolve_placement(&saved, &ms)?;
    Some((geometry::toggle_rect(&p, window_dip(&s.toggle), core_margin(&s.toggle), &m), m))
}

fn place_toggle(app: &AppHandle) {
    let Some(w) = app.get_webview_window(TOGGLE) else { return };
    if let Some((r, _)) = toggle_rect_now(app) {
        state(app).ignore_moves_until.store(now_ms() + 400, SeqCst);
        set_rect(&w, r);
    }
}

fn save(app: &AppHandle, s: Settings) -> Result<(), String> {
    let st = state(app);
    let s = s.clamped();
    s.save(&st.path).map_err(|e| format!("cannot save settings: {e}"))?;
    *st.settings.write().unwrap_or_else(|e| e.into_inner()) = s.clone();
    let _ = app.emit("settings-changed", &s);
    Ok(())
}

/// Called once the user stopped dragging: snap to edge/corner, persist, re-apply.
fn finish_drag(app: &AppHandle) {
    let Some(w) = app.get_webview_window(TOGGLE) else { return };
    let (Ok(pos), Ok(size)) = (w.outer_position(), w.outer_size()) else { return };
    let s = settings(app);
    let r = PRect { x: pos.x, y: pos.y, w: size.width as i32, h: size.height as i32 };
    if let Some(p) = geometry::placement_from_rect(r, window_dip(&s.toggle), core_margin(&s.toggle), s.toggle.snap_threshold, &monitors(app)) {
        let mut n = s; n.placement = Some(p);
        if let Err(e) = save(app, n) { eprintln!("{e}"); }
        place_toggle(app);
    }
}

fn create_toggle(app: &AppHandle) -> Result<(), String> {
    let s = settings(app);
    let d = window_dip(&s.toggle);
    let w = WebviewWindowBuilder::new(app, TOGGLE, WebviewUrl::App("index.html?w=toggle".into()))
        .title("Veyra").decorations(false).transparent(true).always_on_top(s.toggle.always_on_top).skip_taskbar(true)
        .resizable(false).shadow(false).focused(false).visible(false).inner_size(d, d)
        .build().map_err(|e| e.to_string())?;
    let app2 = app.clone();
    w.on_window_event(move |ev| match ev {
        WindowEvent::Moved(_) => {
            let st = state(&app2);
            if now_ms() < st.ignore_moves_until.load(SeqCst) { return; }
            let g = st.move_gen.fetch_add(1, SeqCst) + 1;
            let app3 = app2.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(220)); // no new move for 220 ms: the drag is over
                if state(&app3).move_gen.load(SeqCst) == g { finish_drag(&app3); }
            });
        }
        WindowEvent::ScaleFactorChanged { .. } => place_toggle(&app2),
        _ => {}
    });
    place_toggle(app);
    let _ = w.show();
    Ok(())
}

fn panel_size(s: &Settings, collapsed: bool) -> (f64, f64) { (s.panel_width, if collapsed { COMPACT_H } else { s.panel_height }) }

fn place_panel(app: &AppHandle, w: &WebviewWindow) {
    let s = settings(app);
    let Some((tr, m)) = toggle_rect_now(app) else { return };
    let (pw, ph) = panel_size(&s, state(app).panel_collapsed.load(SeqCst));
    let r = geometry::panel_rect(tr, pw, ph, 8.0, &m);
    set_rect(w, r);
}

fn ensure_panel(app: &AppHandle) -> Result<WebviewWindow, String> {
    if let Some(w) = app.get_webview_window(PANEL) { return Ok(w); }
    let s = settings(app);
    WebviewWindowBuilder::new(app, PANEL, WebviewUrl::App("index.html?w=panel".into()))
        .title("Veyra").decorations(false).transparent(true).always_on_top(s.panel_always_on_top).skip_taskbar(true)
        .resizable(true).min_inner_size(320.0, COMPACT_H).shadow(true).visible(false).focused(false)
        .inner_size(s.panel_width, s.panel_height).build().map_err(|e| e.to_string())
}

fn show_panel(app: &AppHandle) {
    match ensure_panel(app) {
        Ok(w) => { state(app).panel_collapsed.store(false, SeqCst); place_panel(app, &w); let _ = w.show(); let _ = w.set_focus(); let _ = app.emit("panel-visibility", true); let _ = app.emit("panel-collapsed", false); }
        Err(e) => eprintln!("cannot create panel: {e}"),
    }
}
fn hide_panel(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(PANEL) { let _ = w.hide(); }
    let _ = app.emit("panel-visibility", false);
}
fn panel_visible(app: &AppHandle) -> bool { app.get_webview_window(PANEL).and_then(|w| w.is_visible().ok()).unwrap_or(false) }

#[tauri::command] fn get_settings(app: AppHandle) -> Settings { settings(&app) }

#[tauri::command]
fn update_settings(app: AppHandle, new: Settings) -> Result<Settings, String> {
    let old = settings(&app);
    let mut n = new; n.placement = old.placement.clone(); // placement is only changed by dragging or reset
    save(&app, n)?;
    let s = settings(&app);
    if let Some(w) = app.get_webview_window(TOGGLE) {
        let d = window_dip(&s.toggle);
        let _ = w.set_always_on_top(s.toggle.always_on_top);
        let _ = w.set_size(Size::Logical(tauri::LogicalSize::new(d, d)));
        place_toggle(&app);
    }
    if let Some(w) = app.get_webview_window(PANEL) { let _ = w.set_always_on_top(s.panel_always_on_top); }
    Ok(s)
}

#[tauri::command]
fn reset_toggle_position(app: AppHandle) -> Result<(), String> {
    let mut s = settings(&app); s.placement = None; save(&app, s)?; place_toggle(&app); Ok(())
}

#[tauri::command]
fn toggle_clicked(app: AppHandle) {
    if !panel_visible(&app) { show_panel(&app); return; }
    match settings(&app).toggle.second_click {
        SecondClick::Hide => hide_panel(&app),
        SecondClick::Collapse => {
            let st = state(&app);
            let now = !st.panel_collapsed.load(SeqCst);
            st.panel_collapsed.store(now, SeqCst);
            if let Some(w) = app.get_webview_window(PANEL) { place_panel(&app, &w); }
            let _ = app.emit("panel-collapsed", now);
        }
    }
}
#[tauri::command] fn hide_panel_cmd(app: AppHandle) { hide_panel(&app) }
#[tauri::command] fn quit_app(app: AppHandle) { app.exit(0) }

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _a, _c| show_panel(app)))
        .invoke_handler(tauri::generate_handler![get_settings, update_settings, reset_toggle_position, toggle_clicked, quit_app, hide_panel_cmd])
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory); // toggle-only app: no Dock icon
            let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
            let path = dir.join("settings.json");
            let s = Settings::load(&path);
            app.manage::<S>(Arc::new(AppState { path, settings: RwLock::new(s), move_gen: AtomicU64::new(0), ignore_moves_until: AtomicU64::new(0), panel_collapsed: AtomicBool::new(false), topology: Mutex::new(String::new()) }));
            let h = app.handle().clone();
            create_toggle(&h)?;
            // Display changes (plug/unplug, resolution, wake from sleep): cheap local comparison every 3 s, no network, no OS hooks needed.
            std::thread::spawn(move || loop {
                std::thread::sleep(Duration::from_secs(3));
                let sig = topology_signature(&monitors(&h));
                let st = state(&h);
                let changed = { let mut t = st.topology.lock().unwrap_or_else(|e| e.into_inner()); let c = !t.is_empty() && *t != sig; *t = sig; c };
                if changed { place_toggle(&h); if panel_visible(&h) { if let Some(w) = h.get_webview_window(PANEL) { place_panel(&h, &w); } } }
            });
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building Veyra")
        .run(|_app, ev| {
            // Closing every window must not quit: Veyra lives in the toggle.
            if let tauri::RunEvent::ExitRequested { api, code, .. } = ev { if code.is_none() { api.prevent_exit(); } }
        });
}
