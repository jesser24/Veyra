//! Toggle placement in physical pixels. The toggle is an edge tab (like a drawer handle): it docks to one of the four
//! edges or four corners, or floats freely as a small pill. Pure functions, unit-tested on any OS.
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

impl Snap {
    /// Edge the tab is attached to (a corner uses the vertical edge of its column). `None` when floating.
    pub fn side(self) -> Option<Side> {
        match self {
            Snap::Free => None,
            Snap::Edge(s) => Some(s),
            Snap::Corner(Corner::TopLeft | Corner::BottomLeft) => Some(Side::Left),
            Snap::Corner(Corner::TopRight | Corner::BottomRight) => Some(Side::Right),
        }
    }
}

/// `fx`/`fy` are the centre of the window as fractions of the monitor work area, so a placement survives resolution and DPI changes.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Placement { pub monitor: String, pub fx: f64, pub fy: f64, pub snap: Snap }

impl Default for Placement {
    fn default() -> Self { Placement { monitor: String::new(), fx: 1.0, fy: 0.5, snap: Snap::Edge(Side::Right) } }
}

/// Window size in logical pixels: `thickness` is perpendicular to the edge (visible tab + extra hit area), `length` along it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dims { pub thickness: f64, pub length: f64 }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PRect { pub x: i32, pub y: i32, pub w: i32, pub h: i32 }

fn unit(v: f64) -> f64 { if v.is_finite() { v.clamp(0.0, 1.0) } else { 0.5 } }
fn px(dip: f64, scale: f64) -> i32 { (dip * scale).round() as i32 }

/// Physical window size for a snap state.
pub fn window_px(snap: Snap, d: Dims, m: &MonitorInfo) -> (i32, i32) {
    let (t, l) = (px(d.thickness, m.scale).max(1), px(d.length, m.scale).max(1));
    let vertical = matches!(snap.side(), Some(Side::Left | Side::Right));
    let (w, h) = if vertical { (t, l) } else { (l, t) };
    (w.min(m.w.max(1)), h.min(m.h.max(1)))
}

fn clamp_i(v: i32, lo: i32, hi: i32) -> i32 { if hi < lo { lo } else { v.clamp(lo, hi) } }

/// Window rectangle for a placement. `margin_dip` is the gap kept along the edge (from corners) and around a floating toggle.
pub fn toggle_rect(p: &Placement, d: Dims, margin_dip: f64, m: &MonitorInfo) -> PRect {
    let (w, h) = window_px(p.snap, d, m);
    let mg = px(margin_dip, m.scale).max(0);
    let cx = m.x as f64 + unit(p.fx) * m.w as f64;
    let cy = m.y as f64 + unit(p.fy) * m.h as f64;
    let fx = clamp_i((cx - w as f64 / 2.0).round() as i32, m.x + mg, m.x + m.w - w - mg);
    let fy = clamp_i((cy - h as f64 / 2.0).round() as i32, m.y + mg, m.y + m.h - h - mg);
    let (left, right, top, bottom) = (m.x, m.x + m.w - w, m.y + mg, m.y + m.h - h - mg);
    let (x, y) = match p.snap {
        Snap::Free => (fx, fy),
        Snap::Edge(Side::Left) => (left, fy),
        Snap::Edge(Side::Right) => (right, fy),
        Snap::Edge(Side::Top) => (fx, m.y),
        Snap::Edge(Side::Bottom) => (fx, m.y + m.h - h),
        Snap::Corner(Corner::TopLeft) => (left, top),
        Snap::Corner(Corner::TopRight) => (right, top),
        Snap::Corner(Corner::BottomLeft) => (left, bottom),
        Snap::Corner(Corner::BottomRight) => (right, bottom),
    };
    PRect { x, y, w, h }
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

/// Turn the rectangle where the user released the toggle into a placement: within `threshold_dip` of a border it docks to
/// that edge, and also within it of a corner (measured along the edge, after the margin) it docks to the corner.
pub fn placement_from_rect(r: PRect, margin_dip: f64, threshold_dip: f64, monitors: &[MonitorInfo]) -> Option<Placement> {
    let (cx, cy) = (r.x + r.w / 2, r.y + r.h / 2);
    let m = monitor_for_point(cx, cy, monitors)?;
    let mg = px(margin_dip, m.scale).max(0);
    let th = px(threshold_dip, m.scale).max(0);
    let d = [(Side::Left, r.x - m.x), (Side::Right, m.x + m.w - (r.x + r.w)), (Side::Top, r.y - m.y), (Side::Bottom, m.y + m.h - (r.y + r.h))];
    let nearest = d.iter().filter(|(_, v)| *v <= th).min_by_key(|(_, v)| *v).map(|(s, _)| *s);
    let (to_l, to_r) = (r.x - m.x - mg, m.x + m.w - (r.x + r.w) - mg);
    let (to_t, to_b) = (r.y - m.y - mg, m.y + m.h - (r.y + r.h) - mg);
    let snap = match nearest {
        None => Snap::Free,
        Some(Side::Left | Side::Right) => {
            let right = nearest == Some(Side::Right);
            if to_t <= th { Snap::Corner(if right { Corner::TopRight } else { Corner::TopLeft }) }
            else if to_b <= th { Snap::Corner(if right { Corner::BottomRight } else { Corner::BottomLeft }) }
            else { Snap::Edge(if right { Side::Right } else { Side::Left }) }
        }
        Some(Side::Top | Side::Bottom) => {
            let bottom = nearest == Some(Side::Bottom);
            if to_l <= th { Snap::Corner(if bottom { Corner::BottomLeft } else { Corner::TopLeft }) }
            else if to_r <= th { Snap::Corner(if bottom { Corner::BottomRight } else { Corner::TopRight }) }
            else { Snap::Edge(if bottom { Side::Bottom } else { Side::Top }) }
        }
    };
    Some(Placement { monitor: m.id.clone(), fx: unit((cx - m.x) as f64 / m.w.max(1) as f64), fy: unit((cy - m.y) as f64 / m.h.max(1) as f64), snap })
}

/// Re-home a saved placement after monitors changed. Never leaves the toggle off-screen; the caller keeps the saved choice.
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
    PRect { x: clamp_i(x, m.x, m.x + m.w - w), y: clamp_i(y, m.y, m.y + m.h - h), w, h }
}

#[cfg(test)]
mod tests {
    use super::*;
    const D: Dims = Dims { thickness: 24.0, length: 72.0 };
    fn mon(id: &str, x: i32, y: i32, w: i32, h: i32, s: f64) -> MonitorInfo { MonitorInfo { id: id.into(), x, y, w, h, scale: s } }
    fn setups() -> Vec<Vec<MonitorInfo>> { vec![
        vec![mon("A", 0, 0, 1366, 728, 1.0)], vec![mon("A", 0, 0, 1920, 1040, 1.25)], vec![mon("A", 0, 0, 3840, 2120, 2.5)],
        vec![mon("P", 0, 0, 2560, 1400, 1.5), mon("S", -1920, 100, 1920, 1040, 1.0)],
        vec![mon("P", 0, 0, 1920, 1040, 1.0), mon("S", 0, -2160, 3840, 2120, 2.5)],
    ]}
    fn inside(r: &PRect, m: &MonitorInfo) -> bool { r.x >= m.x && r.y >= m.y && r.x + r.w <= m.x + m.w && r.y + r.h <= m.y + m.h }
    fn all_snaps() -> Vec<Snap> { vec![Snap::Free, Snap::Edge(Side::Left), Snap::Edge(Side::Right), Snap::Edge(Side::Top), Snap::Edge(Side::Bottom),
        Snap::Corner(Corner::TopLeft), Snap::Corner(Corner::TopRight), Snap::Corner(Corner::BottomLeft), Snap::Corner(Corner::BottomRight)] }

    #[test] fn every_placement_stays_inside_its_monitor() {
        for set in setups() { for m in &set { for snap in all_snaps() { for f in [-3.0, 0.0, 0.3, 1.0, 9.0, f64::NAN] { for margin in [0.0, 12.0, 96.0] {
            let r = toggle_rect(&Placement { monitor: m.id.clone(), fx: f, fy: f, snap }, D, margin, m);
            assert!(inside(&r, m), "{snap:?} {f} {margin}: {r:?} not in {m:?}");
        }}}}}
    }
    #[test] fn edge_tabs_are_flush_with_the_screen_edge_and_oriented() {
        let m = mon("A", 100, 50, 1920, 1040, 1.0);
        let p = |snap| Placement { monitor: "A".into(), fx: 0.5, fy: 0.5, snap };
        let r = toggle_rect(&p(Snap::Edge(Side::Right)), D, 12.0, &m); assert_eq!(r.x + r.w, m.x + m.w); assert_eq!((r.w, r.h), (24, 72));
        let r = toggle_rect(&p(Snap::Edge(Side::Left)), D, 12.0, &m); assert_eq!(r.x, m.x);
        let r = toggle_rect(&p(Snap::Edge(Side::Top)), D, 12.0, &m); assert_eq!(r.y, m.y); assert_eq!((r.w, r.h), (72, 24));
        let r = toggle_rect(&p(Snap::Edge(Side::Bottom)), D, 12.0, &m); assert_eq!(r.y + r.h, m.y + m.h);
    }
    #[test] fn corners_keep_the_margin_along_the_edge() {
        let m = mon("A", 100, 50, 1920, 1040, 1.0);
        let p = |c| Placement { monitor: "A".into(), fx: 0.5, fy: 0.5, snap: Snap::Corner(c) };
        let r = toggle_rect(&p(Corner::TopLeft), D, 20.0, &m); assert_eq!((r.x, r.y), (100, 70));
        let r = toggle_rect(&p(Corner::BottomRight), D, 20.0, &m); assert_eq!((r.x + r.w, r.y + r.h), (m.x + m.w, m.y + m.h - 20));
    }
    #[test] fn release_snaps_to_all_four_edges_and_all_four_corners_else_free() {
        let ms = vec![mon("A", 0, 0, 1920, 1040, 1.0)];
        let at = |x, y, w, h| placement_from_rect(PRect { x, y, w, h }, 12.0, 40.0, &ms).unwrap().snap;
        assert_eq!(at(10, 500, 24, 72), Snap::Edge(Side::Left));
        assert_eq!(at(1900, 500, 24, 72), Snap::Edge(Side::Right));
        assert_eq!(at(900, 5, 72, 24), Snap::Edge(Side::Top));
        assert_eq!(at(900, 1010, 72, 24), Snap::Edge(Side::Bottom));
        assert_eq!(at(5, 15, 24, 72), Snap::Corner(Corner::TopLeft));
        assert_eq!(at(1895, 15, 24, 72), Snap::Corner(Corner::TopRight));
        assert_eq!(at(5, 950, 24, 72), Snap::Corner(Corner::BottomLeft));
        assert_eq!(at(1895, 950, 24, 72), Snap::Corner(Corner::BottomRight));
        assert_eq!(at(700, 400, 72, 24), Snap::Free);
    }
    #[test] fn zero_threshold_snaps_only_on_the_border() {
        let ms = vec![mon("A", 0, 0, 1000, 800, 1.0)];
        assert_eq!(placement_from_rect(PRect { x: 0, y: 300, w: 24, h: 72 }, 12.0, 0.0, &ms).unwrap().snap, Snap::Edge(Side::Left));
        assert_eq!(placement_from_rect(PRect { x: 1, y: 300, w: 24, h: 72 }, 12.0, 0.0, &ms).unwrap().snap, Snap::Free);
    }
    #[test] fn snapped_placement_roundtrips_through_a_rect() {
        for set in setups() { for m in &set { for snap in all_snaps() {
            let p0 = Placement { monitor: m.id.clone(), fx: 0.4, fy: 0.6, snap };
            let r = toggle_rect(&p0, D, 12.0, m);
            let p1 = placement_from_rect(r, 12.0, 8.0, &set).unwrap();
            assert_eq!(p1.monitor, m.id);
            if snap != Snap::Free { assert_eq!(p1.snap.side(), snap.side(), "{snap:?} -> {:?}", p1.snap); }
            assert!(inside(&toggle_rect(&p1, D, 12.0, m), m));
        }}}
    }
    #[test] fn drop_picks_the_monitor_under_the_toggle() {
        let ms = setups().remove(3);
        assert_eq!(placement_from_rect(PRect { x: -500, y: 300, w: 24, h: 72 }, 12.0, 40.0, &ms).unwrap().monitor, "S");
        assert_eq!(placement_from_rect(PRect { x: 800, y: 300, w: 24, h: 72 }, 12.0, 40.0, &ms).unwrap().monitor, "P");
        assert!(placement_from_rect(PRect { x: 0, y: 0, w: 24, h: 72 }, 12.0, 40.0, &[]).is_none());
    }
    #[test] fn unplugged_monitor_falls_back_without_going_off_screen() {
        let one = vec![mon("P", 0, 0, 1920, 1040, 1.0)];
        let saved = Placement { monitor: "GONE".into(), fx: 1.0, fy: 1.0, snap: Snap::Corner(Corner::BottomRight) };
        let (p, m) = resolve_placement(&saved, &one).unwrap();
        assert_eq!(p.monitor, "P"); assert!(inside(&toggle_rect(&p, D, 12.0, &m), &m));
        assert!(resolve_placement(&saved, &[]).is_none());
    }
    #[test] fn panel_opens_towards_the_screen_centre_and_stays_inside() {
        for set in setups() { for m in &set { for snap in all_snaps() {
            let t = toggle_rect(&Placement { monitor: m.id.clone(), fx: 0.5, fy: 0.5, snap }, D, 12.0, m);
            assert!(inside(&panel_rect(t, 420.0, 620.0, 8.0, m), m), "{snap:?}");
        }}}
        let m = mon("A", 0, 0, 1920, 1040, 1.0);
        let t = toggle_rect(&Placement { monitor: "A".into(), fx: 0.0, fy: 0.5, snap: Snap::Edge(Side::Left) }, D, 12.0, &m);
        assert!(panel_rect(t, 420.0, 620.0, 8.0, &m).x >= t.x + t.w);
    }
    #[test] fn side_of_a_corner_is_its_vertical_edge() {
        assert_eq!(Snap::Corner(Corner::BottomLeft).side(), Some(Side::Left)); assert_eq!(Snap::Free.side(), None);
    }
}
