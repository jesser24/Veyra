import type { Settings, Theme, UiSettings } from "../lib/types";
import { Button, Card, Notice, Segmented, Setting, Slider, Switch } from "../components/ui";

export function AppearanceSection({ s, push }: { s: Settings; push: (n: Settings) => void }) {
  const ui = (p: Partial<UiSettings>) => push({ ...s, ui: { ...s.ui, ...p } });
  return (
    <div className="stack">
      <Card title="Theme">
        <Setting title="Colour scheme" hint="“Automatic” follows the operating system."><Segmented<Theme> label="Theme" value={s.theme} onChange={(v) => push({ ...s, theme: v })} options={[["system", "Automatic"], ["light", "Light"], ["dark", "Dark"]]} /></Setting>
        <Setting title="Accent colour" hint="Used by buttons, selection and focus in both windows."><span className="inline"><input type="color" aria-label="Accent colour" value={s.ui.accent ?? s.toggle.accent} onChange={(e) => ui({ accent: e.target.value })} />{s.ui.accent && <Button variant="quiet" onClick={() => ui({ accent: null })}>Same as the toggle</Button>}</span></Setting>
      </Card>
      <Card title="Comfort">
        <Setting title="Text size"><Slider label="Text size" min={0.85} max={1.3} step={0.05} value={s.ui.text_size} onChange={(v) => ui({ text_size: v })} format={(v) => `${Math.round(v * 100)} %`} /></Setting>
        <Setting title="Density" hint="Spacing between elements."><Segmented label="Density" value={s.ui.density} onChange={(v) => ui({ density: v })} options={[["comfortable", "Comfortable"], ["compact", "Compact"]]} /></Setting>
        <Setting title="Window opacity" hint="Never below 88 % so text always stays readable over any background."><Slider label="Window opacity" min={0.88} max={1} step={0.01} value={s.ui.surface_opacity} onChange={(v) => ui({ surface_opacity: v })} format={(v) => `${Math.round(v * 100)} %`} /></Setting>
        <Setting title="Animations" hint="Slides, fades and the toggle's movements. Also off when the system asks for reduced motion."><Switch label="Animations" checked={s.toggle.animations} onChange={(v) => push({ ...s, toggle: { ...s.toggle, animations: v } })} /></Setting>
        <Setting title="Enter sends the message" hint={s.ui.enter_sends ? "Shift+Enter adds a new line." : "Ctrl/Cmd+Enter sends; Enter adds a new line."}><Switch label="Enter sends" checked={s.ui.enter_sends} onChange={(v) => ui({ enter_sends: v })} /></Setting>
        <Setting title="Chat window always on top"><Switch label="Always on top" checked={s.panel_always_on_top} onChange={(v) => push({ ...s, panel_always_on_top: v })} /></Setting>
      </Card>
      <Notice kind="info">Native window materials (Windows Mica/Acrylic, macOS vibrancy) are not used: Veyra's windows are deliberately near-opaque so text is readable on any desktop. Opacity above is the only transparency control.</Notice>
    </div>
  );
}
