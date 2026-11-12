//! Toggle placement in physical pixels: free position, snapping to edges and corners, margins, multi-monitor.
use serde::{Deserialize, Serialize};

/// A monitor's work area (taskbar/menu bar excluded) in physical pixels on the virtual desktop.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct MonitorInfo { pub id: String, pub x: i32, pub y: i32, pub w: i32, pub h: i32, pub scale: f64 }

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Side { Left, Right, Top, Bottom }

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Corner { TopLeft, TopRight, BottomLeft, BottomRight }

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(tag = "kind", content = "at", rename_all = "snake_case")]
pub enum Snap { Free, Edge(Side), Corner(Corner) }

/// Where the toggle lives. `fx`/`fy` are 0..1 positions inside the free travel range of the monitor
/// (work area minus toggle size minus margins), so they survive resolution and DPI changes.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Placement { pub monitor: String, pub fx: f64, pub fy: f64, pub snap: Snap }

impl Default for Placement {
    fn default() -> Self { Placement { monitor: String::new(), fx: 1.0, fy: 0.5, snap: Snap::Edge(Side::Right) } }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PRect { pub x: i32, pub y: i32, pub w: i32, pub h: i32 }

fn unit(v: f64) -> f64 { if v.is_finite() { v.clamp(0.0, 1.0) } else { 0.5 } }
fn px(dip: f64, scale: f64) -> i32 { (dip * scale).round() as i32 }

/// Window rectangle of the toggle for a placement. `size_dip` and `margin_dip` are logical pixels.
/// A negative margin is allowed: the transparent halo around the visible toggle may extend past the work area.
pub fn toggle_rect(p: &Placement, size_dip: f64, margin_dip: f64, m: &MonitorInfo) -> PRect {
    let size = px(size_dip, m.scale).clamp(1, m.w.min(m.h).max(1));
    let margin = px(margin_dip, m.scale);
    let free_w = (m.w - size - 2 * margin).max(0) as f64;
    let free_h = (m.h - size - 2 * margin).max(0) as f64;
    let (fx, fy) = match p.snap {
        Snap::Free => (unit(p.fx), unit(p.fy)),
        Snap::Edge(Side::Left) => (0.0, unit(p.fy)),
        Snap::Edge(Side::Right) => (1.0, unit(p.fy)),
        Snap::Edge(Side::Top) => (unit(p.fx), 0.0),
        Snap::Edge(Side::Bottom) => (unit(p.fx), 1.0),
        Snap::Corner(Corner::TopLeft) => (0.0, 0.0),
        Snap::Corner(Corner::TopRight) => (1.0, 0.0),
        Snap::Corner(Corner::BottomLeft) => (0.0, 1.0),
        Snap::Corner(Corner::BottomRight) => (1.0, 1.0),
    };
    PRect { x: m.x + margin + (fx * free_w).round() as i32, y: m.y + margin + (fy * free_h).round() as i32, w: size, h: size }
}

/// Monitor containing the point, else the nearest one. `None` only when there are no monitors.
pub fn monitor_for_point(x: i32, y: i32, monitors: &[MonitorInfo]) -> Option<&MonitorInfo> {
    let dist = |m: &MonitorInfo| {
        let dx = (m.x - x).max(0).max(x - (m.x + m.w - 1)) as i64;
        let dy = (m.y - y).max(0).max(y - (m.y + m.h - 1)) as i64;
        dx * dx + dy * dy
    };
    monitors.iter().min_by_key(|m| dist(m))
}

/// Turn the rectangle where the user dropped the toggle into a placement, snapping to an edge or a corner
/// when it ends within `threshold_dip` of the (margin-inset) border. Snap distances are measured on the window rectangle.
pub fn placement_from_rect(r: PRect, size_dip: f64, margin_dip: f64, threshold_dip: f64, monitors: &[MonitorInfo]) -> Option<Placement> {
    let m = monitor_for_point(r.x + r.w / 2, r.y + r.h / 2, monitors)?;
    let margin = px(margin_dip, m.scale);
    let th = px(threshold_dip, m.scale).max(0);
    let size = px(size_dip, m.scale).clamp(1, m.w.min(m.h).max(1));
    let free_w = (m.w - size - 2 * margin).max(0);
    let free_h = (m.h - size - 2 * margin).max(0);
    let rel_x = (r.x - m.x - margin).clamp(0, free_w);
    let rel_y = (r.y - m.y - margin).clamp(0, free_h);
    let fx = if free_w == 0 { 0.5 } else { rel_x as f64 / free_w as f64 };
    let fy = if free_h == 0 { 0.5 } else { rel_y as f64 / free_h as f64 };
    let (l, rt, t, b) = (rel_x <= th, free_w - rel_x <= th, rel_y <= th, free_h - rel_y <= th);
    let snap = match (l, rt, t, b) {
        (true, _, true, _) => Snap::Corner(Corner::TopLeft),
        (_, true, true, _) => Snap::Corner(Corner::TopRight),
        (true, _, _, true) => Snap::Corner(Corner::BottomLeft),
        (_, true, _, true) => Snap::Corner(Corner::BottomRight),
        (true, ..) => Snap::Edge(Side::Left),
        (_, true, ..) => Snap::Edge(Side::Right),
        (_, _, true, _) => Snap::Edge(Side::Top),
        (_, _, _, true) => Snap::Edge(Side::Bottom),
        _ => Snap::Free,
    };
    Some(Placement { monitor: m.id.clone(), fx: unit(fx), fy: unit(fy), snap })
}

/// Re-home a saved placement after monitors changed. Never leaves the toggle off-screen; the saved choice is not overwritten by the caller.
pub fn resolve_placement(saved: &Placement, monitors: &[MonitorInfo]) -> Option<(Placement, MonitorInfo)> {
    let m = monitors.iter().find(|m| m.id == saved.monitor).or_else(|| monitors.first())?;
    Some((Placement { monitor: m.id.clone(), ..saved.clone() }, m.clone()))
}

/// Panel (chat window) rectangle next to the toggle, opening towards the middle of the monitor and kept fully inside it.
pub fn panel_rect(toggle: PRect, panel_w_dip: f64, panel_h_dip: f64, gap_dip: f64, m: &MonitorInfo) -> PRect {
    let w = px(panel_w_dip, m.scale).clamp(1, m.w.max(1));
    let h = px(panel_h_dip, m.scale).clamp(1, m.h.max(1));
    let gap = px(gap_dip, m.scale).max(0);
    let (cx, cy) = (toggle.x + toggle.w / 2, toggle.y + toggle.h / 2);
    let on_left = cx < m.x + m.w / 2;
    let on_top = cy < m.y + m.h / 2;
    let x = if on_left { toggle.x + toggle.w + gap } else { toggle.x - gap - w };
    let y = if on_top { toggle.y } else { toggle.y + toggle.h - h };
    PRect { x: x.clamp(m.x, (m.x + m.w - w).max(m.x)), y: y.clamp(m.y, (m.y + m.h - h).max(m.y)), w, h }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn mon(id: &str, x: i32, y: i32, w: i32, h: i32, s: f64) -> MonitorInfo { MonitorInfo { id: id.into(), x, y, w, h, scale: s } }
    fn setups() -> Vec<Vec<MonitorInfo>> { vec![
        vec![mon("A", 0, 0, 1366, 728, 1.0)], vec![mon("A", 0, 0, 1920, 1040, 1.25)], vec![mon("A", 0, 0, 3840, 2120, 2.5)],
        vec![mon("P", 0, 0, 2560, 1400, 1.5), mon("S", -1920, 100, 1920, 1040, 1.0)],
        vec![mon("P", 0, 0, 1920, 1040, 1.0), mon("S", 0, -2160, 3840, 2120, 2.5)],
    ]}
    fn inside(r: &PRect, m: &MonitorInfo) -> bool { r.x >= m.x && r.y >= m.y && r.x + r.w <= m.x + m.w && r.y + r.h <= m.y + m.h }

    #[test] fn every_placement_stays_inside_its_monitor() {
        let snaps = [Snap::Free, Snap::Edge(Side::Left), Snap::Edge(Side::Right), Snap::Edge(Side::Top), Snap::Edge(Side::Bottom),
            Snap::Corner(Corner::TopLeft), Snap::Corner(Corner::TopRight), Snap::Corner(Corner::BottomLeft), Snap::Corner(Corner::BottomRight)];
        for set in setups() { for m in &set { for snap in snaps { for f in [-3.0, 0.0, 0.3, 1.0, 9.0, f64::NAN] { for (size, margin) in [(52.0, 12.0), (96.0, 64.0), (24.0, 0.0)] {
            let r = toggle_rect(&Placement { monitor: m.id.clone(), fx: f, fy: f, snap }, size, margin, m);
            assert!(inside(&r, m), "{snap:?} {f} {size} {margin}: {r:?} not in {m:?}");
        }}}}}
    }
    #[test] fn margin_is_respected_on_edges_and_corners() {
        let m = mon("A", 100, 50, 1920, 1040, 1.0);
        let r = toggle_rect(&Placement { monitor: "A".into(), fx: 0.0, fy: 0.0, snap: Snap::Corner(Corner::TopLeft) }, 50.0, 20.0, &m);
        assert_eq!((r.x, r.y), (120, 70));
        let r = toggle_rect(&Placement { monitor: "A".into(), fx: 0.0, fy: 0.5, snap: Snap::Edge(Side::Right) }, 50.0, 20.0, &m);
        assert_eq!(r.x + r.w, m.x + m.w - 20);
    }
    #[test] fn drop_near_border_snaps_to_edge_and_corner_else_stays_free() {
        let ms = vec![mon("A", 0, 0, 1920, 1040, 1.0)];
        let p = placement_from_rect(PRect { x: 20, y: 400, w: 52, h: 52 }, 52.0, 12.0, 40.0, &ms).unwrap();
        assert_eq!(p.snap, Snap::Edge(Side::Left));
        let p = placement_from_rect(PRect { x: 1860, y: 970, w: 52, h: 52 }, 52.0, 12.0, 40.0, &ms).unwrap();
        assert_eq!(p.snap, Snap::Corner(Corner::BottomRight));
        let p = placement_from_rect(PRect { x: 700, y: 400, w: 52, h: 52 }, 52.0, 12.0, 40.0, &ms).unwrap();
        assert_eq!(p.snap, Snap::Free);
    }
    #[test] fn snap_threshold_zero_snaps_only_when_exactly_on_the_border() {
        let ms = vec![mon("A", 0, 0, 1000, 800, 1.0)];
        assert_eq!(placement_from_rect(PRect { x: 12, y: 300, w: 50, h: 50 }, 50.0, 12.0, 0.0, &ms).unwrap().snap, Snap::Edge(Side::Left));
        assert_eq!(placement_from_rect(PRect { x: 13, y: 300, w: 50, h: 50 }, 50.0, 12.0, 0.0, &ms).unwrap().snap, Snap::Free);
    }
    #[test] fn drop_roundtrip_is_stable() {
        for set in setups() { for m in &set {
            let p0 = Placement { monitor: m.id.clone(), fx: 0.37, fy: 0.61, snap: Snap::Free };
            let r = toggle_rect(&p0, 52.0, 12.0, m);
            let p1 = placement_from_rect(r, 52.0, 12.0, 0.0, &set).unwrap();
            assert_eq!(p1.monitor, m.id);
            assert!((p1.fx - 0.37).abs() < 0.01 && (p1.fy - 0.61).abs() < 0.01, "{p1:?}");
        }}
    }
    #[test] fn drop_picks_the_monitor_under_the_toggle() {
        let ms = setups().remove(3);
        assert_eq!(placement_from_rect(PRect { x: -500, y: 300, w: 50, h: 50 }, 50.0, 12.0, 40.0, &ms).unwrap().monitor, "S");
        assert_eq!(placement_from_rect(PRect { x: 800, y: 300, w: 50, h: 50 }, 50.0, 12.0, 40.0, &ms).unwrap().monitor, "P");
        assert!(placement_from_rect(PRect { x: 0, y: 0, w: 50, h: 50 }, 50.0, 12.0, 40.0, &[]).is_none());
    }
    #[test] fn unplugged_monitor_falls_back_without_going_off_screen() {
        let one = vec![mon("P", 0, 0, 1920, 1040, 1.0)];
        let saved = Placement { monitor: "GONE".into(), fx: 1.0, fy: 1.0, snap: Snap::Corner(Corner::BottomRight) };
        let (p, m) = resolve_placement(&saved, &one).unwrap();
        assert_eq!(p.monitor, "P");
        assert!(inside(&toggle_rect(&p, 52.0, 12.0, &m), &m));
        assert!(resolve_placement(&saved, &[]).is_none());
    }
    #[test] fn panel_opens_towards_the_screen_centre_and_stays_inside() {
        for set in setups() { for m in &set { for snap in [Snap::Corner(Corner::TopLeft), Snap::Corner(Corner::BottomRight), Snap::Edge(Side::Right), Snap::Edge(Side::Left), Snap::Free] {
            let t = toggle_rect(&Placement { monitor: m.id.clone(), fx: 0.5, fy: 0.5, snap }, 52.0, 12.0, m);
            let p = panel_rect(t, 420.0, 600.0, 10.0, m);
            assert!(inside(&p, m), "{snap:?} {p:?} {m:?}");
        }}}
        let m = mon("A", 0, 0, 1920, 1040, 1.0);
        let t = toggle_rect(&Placement { monitor: "A".into(), fx: 0.0, fy: 0.0, snap: Snap::Corner(Corner::TopLeft) }, 52.0, 12.0, &m);
        assert!(panel_rect(t, 420.0, 600.0, 10.0, &m).x >= t.x + t.w, "opens to the right of a left-hand toggle");
    }
}
