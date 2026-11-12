import { useEffect, useState, type ReactNode } from "react";
import { api } from "./lib/api";
import { useSettings } from "./lib/useSettings";
import { clamp } from "./lib/theme";
import type { Settings, Shape, ToggleSettings } from "./lib/types";

function Row({ label, hint, children }: { label: string; hint?: string; children: ReactNode }) {
  return <label className="row"><span className="lbl">{label}{hint && <small>{hint}</small>}</span>{children}</label>;
}

export function Panel() {
  const s = useSettings();
  const [collapsed, setCollapsed] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  useEffect(() => { let off: (() => void) | undefined; api.onCollapsed(setCollapsed).then((f) => { off = f; }); return () => off?.(); }, []);
  useEffect(() => { const k = (e: KeyboardEvent) => { if (e.key === "Escape") void api.hidePanel(); }; window.addEventListener("keydown", k); return () => window.removeEventListener("keydown", k); }, []);
  if (!s) return null;
  const push = (n: Settings) => api.updateSettings(n).then(() => setErr(null)).catch((e) => setErr(String(e)));
  const setT = (p: Partial<ToggleSettings>) => push({ ...s, toggle: { ...s.toggle, ...p } });
  const num = (k: "size" | "opacity" | "margin" | "snap_threshold", min: number, max: number, step: number, fmt: (v: number) => string) => (
    <span className="ctl"><input type="range" min={min} max={max} step={step} value={s.toggle[k]} onChange={(e) => setT({ [k]: clamp(Number(e.target.value), min, max) })} /><output>{fmt(s.toggle[k])}</output></span>
  );
  return (
    <div className="panel" role="application" aria-label="Veyra">
      <header data-tauri-drag-region>
        <img src="/veyra-mark.png" alt="" draggable={false} data-tauri-drag-region />
        <strong data-tauri-drag-region>Veyra</strong>
        <span className="sp" data-tauri-drag-region />
        <button className="icon" aria-label="Close" title="Close (Esc)" onClick={() => void api.hidePanel()}>✕</button>
      </header>
      {!collapsed && (
        <main>
          <section className="note" role="status">
            <b>Local AI chat is not installed yet.</b> The conversation window, the model manager (llama.cpp + Qwen3) and the encrypted history are the next development phases. Below are the settings that already work.
          </section>
          <h2>Toggle</h2>
          <Row label="Size">{num("size", 28, 120, 1, (v) => `${v} px`)}</Row>
          <Row label="Opacity" hint="never fully invisible">{num("opacity", 0.15, 1, 0.01, (v) => `${Math.round(v * 100)} %`)}</Row>
          <Row label="Screen margin">{num("margin", 0, 96, 1, (v) => `${v} px`)}</Row>
          <Row label="Snap distance" hint="edges and corners">{num("snap_threshold", 0, 120, 1, (v) => `${v} px`)}</Row>
          <Row label="Shape"><select value={s.toggle.shape} onChange={(e) => setT({ shape: e.target.value as Shape })}><option value="circle">Circle</option><option value="squircle">Rounded square</option><option value="pill">Pill</option></select></Row>
          <Row label="Accent colour"><input type="color" value={s.toggle.accent} onChange={(e) => setT({ accent: e.target.value })} /></Row>
          <Row label="Shadow"><input type="checkbox" checked={s.toggle.shadow} onChange={(e) => setT({ shadow: e.target.checked })} /></Row>
          <Row label="Reflection"><input type="checkbox" checked={s.toggle.reflection} onChange={(e) => setT({ reflection: e.target.checked })} /></Row>
          <Row label="Animations"><input type="checkbox" checked={s.toggle.animations} onChange={(e) => setT({ animations: e.target.checked })} /></Row>
          <Row label="Toggle always on top"><input type="checkbox" checked={s.toggle.always_on_top} onChange={(e) => setT({ always_on_top: e.target.checked })} /></Row>
          <Row label="Second click"><select value={s.toggle.second_click} onChange={(e) => setT({ second_click: e.target.value as ToggleSettings["second_click"] })}><option value="hide">Hide the window</option><option value="collapse">Collapse to a bar</option></select></Row>
          <Row label="Position"><button onClick={() => void api.resetTogglePosition()}>Reset to right edge</button></Row>
          <h2>Window</h2>
          <Row label="Theme"><select value={s.theme} onChange={(e) => push({ ...s, theme: e.target.value as Settings["theme"] })}><option value="system">Automatic</option><option value="light">Light</option><option value="dark">Dark</option></select></Row>
          <Row label="Window always on top"><input type="checkbox" checked={s.panel_always_on_top} onChange={(e) => push({ ...s, panel_always_on_top: e.target.checked })} /></Row>
          {err && <p className="err" role="alert">{err}</p>}
          <footer><span>Network: none. No telemetry.</span><button onClick={() => void api.quit()}>Quit Veyra</button></footer>
        </main>
      )}
    </div>
  );
}
