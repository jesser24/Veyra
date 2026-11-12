import { useEffect, useRef, useState, type CSSProperties, type KeyboardEvent, type PointerEvent } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { api } from "./lib/api";
import { useSettings } from "./lib/useSettings";
import { tabRadius } from "./lib/theme";
import type { Side } from "./lib/types";

const DRAG_THRESHOLD = 4;

/** The edge tab. The whole window is the hit area; the drawn tab sits flush against the screen edge it is docked to.
 *  A press without movement is a click; movement hands the drag to the OS (correct across monitors and DPI). */
export function Toggle() {
  const s = useSettings();
  const [side, setSide] = useState<Side | null>("right");
  const down = useRef<{ x: number; y: number; dragging: boolean } | null>(null);
  useEffect(() => {
    let off: (() => void) | undefined;
    api.toggleVisual().then(setSide).catch((e) => api.logUi(`toggleVisual failed: ${String(e)}`));
    api.onVisual(setSide).then((f) => { off = f; });
    return () => off?.();
  }, []);
  if (!s) return null;
  const t = s.toggle;
  const vertical = side === "left" || side === "right";
  const style = { "--thick": `${t.thickness}px`, "--accent": t.accent, "--op": t.opacity, "--radius": tabRadius(t.shape, side, t.thickness) } as CSSProperties;
  const click = () => { api.toggleClicked().catch((e) => api.logUi(`toggle_clicked failed: ${String(e)}`)); };
  const onDown = (e: PointerEvent) => {
    if (e.button !== 0) return;
    down.current = { x: e.screenX, y: e.screenY, dragging: false };
    e.currentTarget.setPointerCapture(e.pointerId);
  };
  const onMove = (e: PointerEvent) => {
    const d = down.current;
    if (!d || d.dragging || (e.buttons & 1) === 0) return;
    if (Math.hypot(e.screenX - d.x, e.screenY - d.y) >= DRAG_THRESHOLD) { d.dragging = true; getCurrentWindow().startDragging().catch((err) => api.logUi(`startDragging failed: ${String(err)}`)); }
  };
  const onUp = () => { const d = down.current; down.current = null; if (d && !d.dragging) click(); };
  const onKey = (e: KeyboardEvent) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); click(); } };
  return (
    <button className={`toggle at-${side ?? "free"} ${vertical ? "v" : "h"}`} style={style} aria-label="Open Veyra" title="Veyra — click to open, drag to move"
      onPointerDown={onDown} onPointerMove={onMove} onPointerUp={onUp} onPointerCancel={() => { down.current = null; }} onKeyDown={onKey}>
      <span className={`tab ${t.shadow ? "shadow" : ""} ${t.reflection ? "reflect" : ""}`}><i /><i /><i /></span>
    </button>
  );
}
