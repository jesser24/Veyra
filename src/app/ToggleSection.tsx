import { api } from "../lib/api";
import { tabRadius } from "../lib/theme";
import type { Settings, Shape, ToggleSettings } from "../lib/types";
import { Button, Card, Segmented, Setting, Slider, Switch } from "../components/ui";

function Preview({ t }: { t: ToggleSettings }) {
  const r = tabRadius(t.shape, "right", t.thickness);
  return <div className="tpreview" aria-hidden="true"><div className="screen"><i style={{ width: t.thickness, height: Math.min(86, t.length * 0.55), borderRadius: r, opacity: t.opacity, background: `linear-gradient(160deg, color-mix(in srgb, ${t.accent} 55%, white), color-mix(in srgb, ${t.accent} 85%, transparent) 45%, color-mix(in srgb, ${t.accent} 60%, #05061a))`, boxShadow: t.shadow ? "0 2px 10px rgba(8,10,40,.4)" : "none", marginRight: Math.min(t.margin / 3, 14) }} /></div></div>;
}
export function ToggleSection({ s, push }: { s: Settings; push: (n: Settings) => void }) {
  const t = s.toggle; const set = (p: Partial<ToggleSettings>) => push({ ...s, toggle: { ...t, ...p } });
  const sl = (k: "thickness" | "length" | "opacity" | "margin" | "snap_threshold", label: string, min: number, max: number, step: number, fmt: (v: number) => string) => <Slider label={label} min={min} max={max} step={step} value={t[k]} onChange={(v) => set({ [k]: v })} format={fmt} />;
  return (
    <div className="stack">
      <Card title="The edge tab" hint="Drag it along any edge or corner of any screen; it remembers where you left it. Preview (not to scale):"><Preview t={t} /></Card>
      <Card title="Appearance">
        <Setting title="Shape"><Segmented<Shape> label="Shape" value={t.shape} onChange={(v) => set({ shape: v })} options={[["tab", "Edge tab"], ["rounded", "Rounded"], ["pill", "Pill"]]} /></Setting>
        <Setting title="Thickness">{sl("thickness", "Thickness", 6, 20, 1, (v) => `${v} px`)}</Setting>
        <Setting title="Length">{sl("length", "Length", 40, 200, 1, (v) => `${v} px`)}</Setting>
        <Setting title="Opacity" hint="Never fully invisible.">{sl("opacity", "Opacity", 0.15, 1, 0.01, (v) => `${Math.round(v * 100)} %`)}</Setting>
        <Setting title="Colour"><input type="color" aria-label="Toggle colour" value={t.accent} onChange={(e) => set({ accent: e.target.value })} /></Setting>
        <Setting title="Shadow"><Switch label="Shadow" checked={t.shadow} onChange={(v) => set({ shadow: v })} /></Setting>
        <Setting title="Reflection"><Switch label="Reflection" checked={t.reflection} onChange={(v) => set({ reflection: v })} /></Setting>
      </Card>
      <Card title="Position and behaviour">
        <Setting title="Distance from the screen edge">{sl("margin", "Margin", 0, 96, 1, (v) => `${v} px`)}</Setting>
        <Setting title="Snap distance" hint="How close to an edge or corner it clings.">{sl("snap_threshold", "Snap", 0, 120, 1, (v) => `${v} px`)}</Setting>
        <Setting title="Second click"><Segmented label="Second click" value={t.second_click} onChange={(v) => set({ second_click: v })} options={[["hide", "Hide the chat"], ["collapse", "Collapse to a bar"]]} /></Setting>
        <Setting title="Always on top"><Switch label="Always on top" checked={t.always_on_top} onChange={(v) => set({ always_on_top: v })} /></Setting>
        <Setting title="Position" hint="Back to the middle of the right edge."><Button onClick={() => void api.resetTogglePosition()}>Reset position</Button></Setting>
      </Card>
    </div>
  );
}
