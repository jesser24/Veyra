import { useRef, type CSSProperties, type KeyboardEvent, type PointerEvent } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { api } from "./lib/api";
import { useSettings } from "./lib/useSettings";
import { shapeRadius } from "./lib/theme";

const DRAG_THRESHOLD = 4;

/** The floating toggle. A press without movement is a click; movement hands the drag to the OS (works across monitors and DPI). */
export function Toggle() {
  const s = useSettings();
  const down = useRef<{ x: number; y: number; dragging: boolean } | null>(null);
  if (!s) return null;
  const t = s.toggle;
  const style = { "--size": `${t.size}px`, "--accent": t.accent, "--op": t.opacity, "--radius": shapeRadius(t.shape),
    "--w": t.shape === "pill" ? "100%" : "var(--size)", "--h": t.shape === "pill" ? "calc(var(--size) * 0.62)" : "var(--size)" } as CSSProperties;
  const onDown = (e: PointerEvent) => { if (e.button === 0) down.current = { x: e.screenX, y: e.screenY, dragging: false }; };
  const onMove = (e: PointerEvent) => {
    const d = down.current;
    if (!d || d.dragging || (e.buttons & 1) === 0) return;
    if (Math.hypot(e.screenX - d.x, e.screenY - d.y) >= DRAG_THRESHOLD) { d.dragging = true; void getCurrentWindow().startDragging(); }
  };
  const onUp = () => { const d = down.current; down.current = null; if (d && !d.dragging) void api.toggleClicked(); };
  const onKey = (e: KeyboardEvent) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); void api.toggleClicked(); } };
  return (
    <div className="toggle-root" style={style}>
      <button className={`toggle ${t.shadow ? "shadow" : ""} ${t.reflection ? "reflect" : ""}`} aria-label="Open Veyra" title="Veyra — click to open, drag to move"
        onPointerDown={onDown} onPointerMove={onMove} onPointerUp={onUp} onPointerCancel={() => { down.current = null; }} onKeyDown={onKey}>
        <img src="/veyra-mark.png" alt="" draggable={false} />
      </button>
    </div>
  );
}
