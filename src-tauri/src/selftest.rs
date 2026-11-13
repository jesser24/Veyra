//! Real-window self-test, enabled only by `VEYRA_SELFTEST=1` (used by CI on a real OS; never active for users).
//! It drives the same code paths as a user: docks the toggle on 4 edges + 4 corners, clicks it, checks the panel opens
//! inside the screen, checks both second-click behaviours and a simulated drag release. Writes `VEYRA_SELFTEST_OUT` (JSON).
use crate::*;
use veyra_core::geometry::{Corner, Placement, Snap};
use veyra_core::settings::SecondClick;

fn sleep(ms: u64) { std::thread::sleep(Duration::from_millis(ms)); }
fn within(r: PRect, m: &MonitorInfo) -> bool { r.x >= m.x - 1 && r.y >= m.y - 1 && r.x + r.w <= m.x + m.w + 1 && r.y + r.h <= m.y + m.h + 1 }
fn rect_of(w: &WebviewWindow) -> Option<PRect> { let (p, s) = (w.outer_position().ok()?, w.outer_size().ok()?); Some(PRect { x: p.x, y: p.y, w: s.width as i32, h: s.height as i32 }) }

pub fn spawn(app: AppHandle) {
    std::thread::spawn(move || {
        sleep(2500);
        let mut results: Vec<(String, bool, String)> = vec![];
        let mut check = |name: &str, ok: bool, detail: String| { log(&app, &format!("selftest {name}: {ok} {detail}")); results.push((name.to_string(), ok, detail)); };
        let ms = monitors(&app);
        check("monitors found", !ms.is_empty(), format!("{ms:?}"));
        let m = ms.first().cloned();
        let t = app.get_webview_window(TOGGLE);
        check("toggle window exists", t.is_some(), String::new());
        if let (Some(m), Some(t)) = (m.clone(), t.clone()) {
            let snaps = [("left", Snap::Edge(Side::Left)), ("right", Snap::Edge(Side::Right)), ("top", Snap::Edge(Side::Top)), ("bottom", Snap::Edge(Side::Bottom)),
                ("top-left", Snap::Corner(Corner::TopLeft)), ("top-right", Snap::Corner(Corner::TopRight)), ("bottom-left", Snap::Corner(Corner::BottomLeft)), ("bottom-right", Snap::Corner(Corner::BottomRight)), ("free", Snap::Free)];
            for (name, snap) in snaps {
                let mut s = settings(&app); s.placement = Some(Placement { monitor: m.id.clone(), fx: 0.5, fy: 0.5, snap });
                let _ = save(&app, s); place_toggle(&app); sleep(500);
                let want = toggle_rect_now(&app).map(|x| x.0);
                let got = rect_of(&t);
                let ok = matches!((want, got), (Some(a), Some(b)) if within(b, &m) && (a.x - b.x).abs() <= 2 && (a.y - b.y).abs() <= 2);
                check(&format!("toggle placed at {name}"), ok, format!("want {want:?} got {got:?}"));
            }
            // click -> panel opens near the toggle, inside the screen
            let mut s = settings(&app); s.placement = Some(Placement { monitor: m.id.clone(), fx: 1.0, fy: 0.5, snap: Snap::Edge(Side::Right) }); s.toggle.second_click = SecondClick::Hide;
            let _ = save(&app, s); place_toggle(&app); sleep(400);
            toggle_clicked_impl(&app); sleep(1500);
            let pw = app.get_webview_window(PANEL);
            let pr = pw.as_ref().and_then(rect_of);
            check("click opens the panel (visible)", panel_visible(&app), format!("{pr:?}"));
            check("panel is inside the screen", matches!(pr, Some(r) if within(r, &m)), format!("{pr:?} in {m:?}"));
            toggle_clicked_impl(&app); sleep(600);
            check("second click hides (Hide mode)", !panel_visible(&app), String::new());
            let mut s = settings(&app); s.toggle.second_click = SecondClick::Collapse; let _ = save(&app, s);
            toggle_clicked_impl(&app); sleep(1000);
            check("click reopens the panel", panel_visible(&app), String::new());
            toggle_clicked_impl(&app); sleep(800);
            let h = pw.as_ref().and_then(rect_of).map(|r| r.h).unwrap_or(0);
            check("second click collapses (Collapse mode)", panel_visible(&app) && (h as f64) < settings(&app).panel_height * m.scale * 0.5, format!("height {h}"));
            toggle_clicked_impl(&app); sleep(800);
            let h2 = pw.as_ref().and_then(rect_of).map(|r| r.h).unwrap_or(0);
            check("third click expands", h2 > h, format!("{h} -> {h2}"));
            check("toggle hidden from the taskbar (WS_EX_TOOLWINDOW; Windows only)", is_tool_window(&t).unwrap_or(true), format!("{:?}", is_tool_window(&t)));
            check("panel hidden from the taskbar (WS_EX_TOOLWINDOW; Windows only)", pw.as_ref().map(|w| is_tool_window(w).unwrap_or(true)).unwrap_or(false), String::new());
            hide_panel(&app); sleep(400);
            // simulated drag release near the left edge -> docks and persists
            let _ = t.set_position(Position::Physical(PhysicalPosition::new(m.x + 6, m.y + m.h / 2)));
            sleep(400); finish_drag(&app); sleep(900);
            let saved = Settings::load(&state(&app).path);
            check("drag release near left edge docks to Left and persists", matches!(saved.placement.map(|p| p.snap), Some(Snap::Edge(Side::Left))), String::new());
        }
        let ok = results.iter().all(|r| r.1);
        let json = serde_json_lines(&results, ok);
        if let Some(p) = std::env::var_os("VEYRA_SELFTEST_OUT") { let _ = std::fs::write(p, json); }
        app.exit(if ok { 0 } else { 1 });
    });
}

fn serde_json_lines(r: &[(String, bool, String)], ok: bool) -> String {
    let items: Vec<String> = r.iter().map(|(n, o, d)| format!("{{\"name\":{:?},\"ok\":{},\"detail\":{:?}}}", n, o, d)).collect();
    format!("{{\"ok\":{},\"checks\":[{}]}}", ok, items.join(","))
}
