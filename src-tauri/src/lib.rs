//! Veyra native shell (Tauri 2): the edge-tab toggle, the panel window and settings persistence.
//! Pure logic (geometry, settings) lives in `veyra-core`. This file makes no network connection of any kind.
mod selftest;

use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::SeqCst};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, Position, Size, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent};
use veyra_core::geometry::{self, Dims, MonitorInfo, PRect, Side};
use veyra_core::settings::{SecondClick, Settings};

/// Windows: WS_EX_TOOLWINDOW keeps a window out of the taskbar and Alt-Tab reliably (Tauri's `skip_taskbar` alone can be lost when a
/// hidden window is shown later). WS_EX_NOACTIVATE additionally stops the toggle from stealing focus when clicked. Same approach as MyPinboard.
#[cfg(windows)]
mod win {
    extern "system" { fn GetWindowLongPtrW(h: isize, i: i32) -> isize; fn SetWindowLongPtrW(h: isize, i: i32, v: isize) -> isize; }
    const GWL_EXSTYLE: i32 = -20; pub const TOOLWINDOW: isize = 0x80; pub const NOACTIVATE: isize = 0x0800_0000;
    pub fn add_ex_style(h: isize, flags: isize) { unsafe { let ex = GetWindowLongPtrW(h, GWL_EXSTYLE); SetWindowLongPtrW(h, GWL_EXSTYLE, ex | flags); } }
    pub fn ex_style(h: isize) -> isize { unsafe { GetWindowLongPtrW(h, GWL_EXSTYLE) } }
}
/// Hide a window from the taskbar / app switcher with native means. Returns false if it could not be verified.
pub(crate) fn hide_from_taskbar(w: &WebviewWindow, no_activate: bool) {
    let _ = w.set_skip_taskbar(true);
    #[cfg(windows)]
    if let Ok(h) = w.hwnd() { win::add_ex_style(h.0 as isize, win::TOOLWINDOW | if no_activate { win::NOACTIVATE } else { 0 }); }
    #[cfg(not(windows))]
    let _ = no_activate;
}
/// Windows only: true when the window carries WS_EX_TOOLWINDOW. Other platforms: `None`.
pub(crate) fn is_tool_window(w: &WebviewWindow) -> Option<bool> {
    #[cfg(windows)] { return w.hwnd().ok().map(|h| win::ex_style(h.0 as isize) & win::TOOLWINDOW != 0); }
    #[cfg(not(windows))] { let _ = w; None }
}

pub(crate) const TOGGLE: &str = "toggle";
pub(crate) const PANEL: &str = "panel";
/// Extra transparent hit area around the drawn tab, perpendicular to the edge (dip).
const HIT_EXTRA: f64 = 14.0;
pub(crate) const COMPACT_H: f64 = 64.0;

pub(crate) struct AppState {
    pub(crate) path: PathBuf,
    pub(crate) log_path: PathBuf,
    pub(crate) settings: RwLock<Settings>,
    move_gen: AtomicU64,
    ignore_moves_until: AtomicU64,
    pub(crate) panel_collapsed: AtomicBool,
    pub(crate) panel_gen: AtomicU64,
    pub(crate) panel_open: AtomicBool,
    topology: Mutex<String>,
    visual: Mutex<Option<Side>>,
}
type S = Arc<AppState>;

fn now_ms() -> u64 { SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0) }
pub(crate) fn state(app: &AppHandle) -> S { app.state::<S>().inner().clone() }
pub(crate) fn settings(app: &AppHandle) -> Settings { state(app).settings.read().unwrap_or_else(|e| e.into_inner()).clone() }

/// Append one line to `<app data>/logs/veyra.log` (kept small). Errors writing the log are ignored on purpose.
pub(crate) fn log(app: &AppHandle, msg: &str) {
    let st = state(app);
    if let Some(d) = st.log_path.parent() { let _ = std::fs::create_dir_all(d); }
    if std::fs::metadata(&st.log_path).map(|m| m.len() > 512_000).unwrap_or(false) { let _ = std::fs::rename(&st.log_path, st.log_path.with_extension("old")); }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&st.log_path) { let _ = writeln!(f, "{} {}", now_ms(), msg); }
}

pub(crate) fn monitors(app: &AppHandle) -> Vec<MonitorInfo> {
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
pub(crate) fn dims(s: &Settings) -> Dims { Dims { thickness: s.toggle.thickness + HIT_EXTRA, length: s.toggle.length } }

pub(crate) fn toggle_rect_now(app: &AppHandle) -> Option<(PRect, MonitorInfo)> {
    let s = settings(app);
    let saved = s.placement.clone().unwrap_or_default();
    let (p, m) = geometry::resolve_placement(&saved, &monitors(app))?;
    Some((geometry::toggle_rect(&p, dims(&s), s.toggle.margin, &m), m))
}

pub(crate) fn place_toggle(app: &AppHandle) {
    let Some(w) = app.get_webview_window(TOGGLE) else { return };
    let s = settings(app);
    let side = s.placement.clone().unwrap_or_default().snap.side();
    *state(app).visual.lock().unwrap_or_else(|e| e.into_inner()) = side;
    if let Some((r, _)) = toggle_rect_now(app) {
        state(app).ignore_moves_until.store(now_ms() + 400, SeqCst);
        set_rect(&w, r);
    }
    let _ = app.emit("toggle-visual", side);
}

/// Slide the toggle to its docked rectangle (ease-out, ~160 ms) instead of jumping. Size (orientation) changes immediately.
fn animate_toggle(app: &AppHandle) {
    let Some(w) = app.get_webview_window(TOGGLE) else { return };
    let Some((to, _)) = toggle_rect_now(app) else { return };
    let Ok(from) = w.outer_position() else { place_toggle(app); return };
    let s = settings(app);
    *state(app).visual.lock().unwrap_or_else(|e| e.into_inner()) = s.placement.clone().unwrap_or_default().snap.side();
    let _ = app.emit("toggle-visual", s.placement.clone().unwrap_or_default().snap.side());
    let _ = w.set_size(Size::Physical(PhysicalSize::new(to.w.max(1) as u32, to.h.max(1) as u32)));
    if !s.toggle.animations { place_toggle(app); return; }
    let (app2, w2) = (app.clone(), w.clone());
    std::thread::spawn(move || {
        let steps = 12u32;
        for i in 1..=steps {
            let t = i as f64 / steps as f64; let e = 1.0 - (1.0 - t).powi(3);
            state(&app2).ignore_moves_until.store(now_ms() + 300, SeqCst);
            let x = from.x as f64 + (to.x - from.x) as f64 * e; let y = from.y as f64 + (to.y - from.y) as f64 * e;
            let _ = w2.set_position(Position::Physical(PhysicalPosition::new(x.round() as i32, y.round() as i32)));
            std::thread::sleep(Duration::from_millis(13));
        }
        place_toggle(&app2); // exact final rectangle
    });
}

pub(crate) fn save(app: &AppHandle, s: Settings) -> Result<(), String> {
    let st = state(app);
    let s = s.clamped();
    s.save(&st.path).map_err(|e| format!("cannot save settings: {e}"))?;
    *st.settings.write().unwrap_or_else(|e| e.into_inner()) = s.clone();
    let _ = app.emit("settings-changed", &s);
    Ok(())
}

/// The user stopped dragging: dock to the nearest edge/corner (within the snap distance), persist, re-apply.
pub(crate) fn finish_drag(app: &AppHandle) {
    let Some(w) = app.get_webview_window(TOGGLE) else { return };
    let (Ok(pos), Ok(size)) = (w.outer_position(), w.outer_size()) else { return };
    let s = settings(app);
    let r = PRect { x: pos.x, y: pos.y, w: size.width as i32, h: size.height as i32 };
    if let Some(p) = geometry::placement_from_rect(r, s.toggle.margin, s.toggle.snap_threshold, &monitors(app)) {
        log(app, &format!("drag finished -> {:?}", p.snap));
        let mut n = s; n.placement = Some(p);
        if let Err(e) = save(app, n) { log(app, &e); }
        animate_toggle(app);
    }
}

fn create_toggle(app: &AppHandle) -> Result<(), String> {
    let s = settings(app);
    let (w0, h0) = (s.toggle.thickness + HIT_EXTRA, s.toggle.length);
    let w = WebviewWindowBuilder::new(app, TOGGLE, WebviewUrl::App("index.html?w=toggle".into()))
        .title("Veyra").decorations(false).transparent(true).always_on_top(s.toggle.always_on_top).skip_taskbar(true)
        .resizable(false).shadow(false).focused(false).visible(false).inner_size(w0, h0)
        .build().map_err(|e| e.to_string())?;
    hide_from_taskbar(&w, true);
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

pub(crate) fn place_panel(app: &AppHandle, w: &WebviewWindow) {
    let s = settings(app);
    let Some((tr, m)) = toggle_rect_now(app) else { return };
    let (pw, ph) = panel_size(&s, state(app).panel_collapsed.load(SeqCst));
    set_rect(w, geometry::panel_rect(tr, pw, ph, 8.0, &m));
}

fn ensure_panel(app: &AppHandle) -> Result<WebviewWindow, String> {
    if let Some(w) = app.get_webview_window(PANEL) { return Ok(w); }
    let s = settings(app);
    WebviewWindowBuilder::new(app, PANEL, WebviewUrl::App("index.html?w=panel".into()))
        .title("Veyra").decorations(false).transparent(true).always_on_top(s.panel_always_on_top).skip_taskbar(true)
        .resizable(true).min_inner_size(320.0, COMPACT_H).shadow(true).visible(false).focused(false)
        .inner_size(s.panel_width, s.panel_height).build().map_err(|e| e.to_string())
        .map(|w| { hide_from_taskbar(&w, false); w })
}

pub(crate) fn show_panel(app: &AppHandle) {
    match ensure_panel(app) {
        Ok(w) => {
            state(app).panel_gen.fetch_add(1, SeqCst); state(app).panel_open.store(true, SeqCst);
            state(app).panel_collapsed.store(false, SeqCst);
            place_panel(app, &w);
            if let Err(e) = w.show() { log(app, &format!("panel show failed: {e}")); }
            let _ = w.unminimize();
            let _ = w.set_focus();
            let _ = app.emit("panel-visibility", true);
            let _ = app.emit("panel-collapsed", false);
            log(app, "panel shown");
        }
        Err(e) => log(app, &format!("cannot create panel: {e}")),
    }
}
pub(crate) fn hide_panel(app: &AppHandle) {
    let gen = state(app).panel_gen.fetch_add(1, SeqCst) + 1; state(app).panel_open.store(false, SeqCst);
    let _ = app.emit("panel-visibility", false); // the UI fades out for ~120 ms, then the window is hidden unless it was re-opened meanwhile
    let app2 = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(120));
        if state(&app2).panel_gen.load(SeqCst) == gen { if let Some(w) = app2.get_webview_window(PANEL) { let _ = w.hide(); } log(&app2, "panel hidden"); }
    });
}
pub(crate) fn panel_visible(app: &AppHandle) -> bool { state(app).panel_open.load(SeqCst) && app.get_webview_window(PANEL).and_then(|w| w.is_visible().ok()).unwrap_or(false) }

/// The toggle was clicked. First click opens the panel; the second follows the "second click" setting.
pub(crate) fn toggle_clicked_impl(app: &AppHandle) {
    log(app, "toggle clicked");
    if !panel_visible(app) { show_panel(app); return; }
    match settings(app).toggle.second_click {
        SecondClick::Hide => hide_panel(app),
        SecondClick::Collapse => {
            let st = state(app);
            let now = !st.panel_collapsed.load(SeqCst);
            st.panel_collapsed.store(now, SeqCst);
            if let Some(w) = app.get_webview_window(PANEL) { place_panel(app, &w); }
            let _ = app.emit("panel-collapsed", now);
        }
    }
}

#[tauri::command] fn get_settings(app: AppHandle) -> Settings { settings(&app) }
#[tauri::command] fn toggle_visual(app: AppHandle) -> Option<Side> { *state(&app).visual.lock().unwrap_or_else(|e| e.into_inner()) }
#[tauri::command] fn log_ui(app: AppHandle, msg: String) { log(&app, &format!("ui: {}", msg.chars().take(300).collect::<String>())) }

/// Window-creating commands MUST be `async`: building a webview window from a synchronous command blocks the main thread on Windows
/// (Tauri documents this deadlock). That was the reason the first build's click did nothing.
#[tauri::command]
async fn toggle_clicked(app: AppHandle) { toggle_clicked_impl(&app); }

#[tauri::command]
async fn update_settings(app: AppHandle, new: Settings) -> Result<Settings, String> {
    let old = settings(&app);
    let mut n = new; n.placement = old.placement.clone(); // placement only changes by dragging or reset
    save(&app, n)?;
    let s = settings(&app);
    if let Some(w) = app.get_webview_window(TOGGLE) { let _ = w.set_always_on_top(s.toggle.always_on_top); place_toggle(&app); }
    if let Some(w) = app.get_webview_window(PANEL) { let _ = w.set_always_on_top(s.panel_always_on_top); }
    Ok(s)
}

#[tauri::command]
async fn reset_toggle_position(app: AppHandle) -> Result<(), String> {
    let mut s = settings(&app); s.placement = None; save(&app, s)?; place_toggle(&app); Ok(())
}
#[tauri::command] async fn hide_panel_cmd(app: AppHandle) { hide_panel(&app) }
#[tauri::command] fn quit_app(app: AppHandle) { app.exit(0) }

mod ai;

#[tauri::command] async fn net_summary(ai: tauri::State<'_, Arc<ai::Ai>>) -> Result<veyra_net::Summary, String> { Ok(ai.net.ledger.summary()) }
#[tauri::command] async fn net_entries(ai: tauri::State<'_, Arc<ai::Ai>>) -> Result<Vec<veyra_net::Entry>, String> { Ok(ai.net.ledger.entries()) }
#[tauri::command] async fn net_clear(ai: tauri::State<'_, Arc<ai::Ai>>) -> Result<(), String> { ai.net.ledger.clear(); Ok(()) }

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _a, _c| { let h = app.clone(); std::thread::spawn(move || show_panel(&h)); }))
        .invoke_handler(tauri::generate_handler![get_settings, toggle_visual, log_ui, update_settings, reset_toggle_position, toggle_clicked, quit_app, hide_panel_cmd,
            net_summary, net_entries, net_clear, ai::ai_overview, ai::ai_download, ai::ai_download_cancel, ai::ai_verify, ai::ai_delete_model, ai::ai_load, ai::ai_unload, ai::ai_status, ai::ai_cancel, ai::ai_test,
            ai::vault_status, ai::vault_create, ai::vault_unlock, ai::vault_unlock_recovery, ai::vault_lock, ai::conv_list, ai::conv_messages, ai::conv_create, ai::conv_rename, ai::conv_archive, ai::conv_trash, ai::conv_restore, ai::conv_delete_permanently, ai::chat_send])
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory); // toggle-only app: no Dock icon
            let dir = std::env::var_os("VEYRA_DATA_DIR").map(PathBuf::from).unwrap_or_else(|| app.path().app_data_dir().expect("app data dir"));
            let s = Settings::load(&dir.join("settings.json"));
            app.manage::<S>(Arc::new(AppState { path: dir.join("settings.json"), log_path: dir.join("logs").join("veyra.log"), settings: RwLock::new(s),
                move_gen: AtomicU64::new(0), ignore_moves_until: AtomicU64::new(0), panel_collapsed: AtomicBool::new(false), panel_gen: AtomicU64::new(0), panel_open: AtomicBool::new(false), topology: Mutex::new(String::new()), visual: Mutex::new(None) }));
            let aiq = Arc::new(ai::Ai::new(dir.clone()).map_err(|e| -> Box<dyn std::error::Error> { e.into() })?);
            app.manage(aiq);
            let h = app.handle().clone();
            log(&h, "Veyra started");
            // an engine left running by a crash would hold gigabytes of RAM: end it before anything else starts
            if let Some(bin) = ai::engine_binary(&h) { if veyra_ai::engine::reap_orphans(&dir.join("models"), &bin) { log(&h, "ended an inference engine left over from a previous run"); } }
            create_toggle(&h)?;
            // Display changes (plug/unplug, resolution, wake from sleep): cheap local comparison every 3 s. No network, no OS hooks needed.
            let hw = h.clone();
            std::thread::spawn(move || loop {
                std::thread::sleep(Duration::from_secs(3));
                let sig = topology_signature(&monitors(&hw));
                let st = state(&hw);
                let changed = { let mut t = st.topology.lock().unwrap_or_else(|e| e.into_inner()); let c = !t.is_empty() && *t != sig; *t = sig; c };
                if changed { place_toggle(&hw); if panel_visible(&hw) { if let Some(w) = hw.get_webview_window(PANEL) { place_panel(&hw, &w); } } }
            });
            if std::env::var_os("VEYRA_SELFTEST").is_some() { selftest::spawn(h); }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building Veyra")
        .run(|app, ev| {
            if let tauri::RunEvent::Exit = ev { // the engine child must never outlive Veyra
                if let Some(ai) = app.try_state::<Arc<ai::Ai>>() { let ai = ai.inner().clone(); tauri::async_runtime::block_on(ai::shutdown(&ai)); }
            }
            // Closing every window must not quit: Veyra lives in the toggle.
            if let tauri::RunEvent::ExitRequested { api, code, .. } = ev { if code.is_none() { api.prevent_exit(); } }
        });
}
