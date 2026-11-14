import { useState } from "react";
import { ai } from "../lib/ai";
import { useEngine } from "../lib/hooks";
import type { AiSettings, Settings } from "../lib/types";
import { Button, Card, Notice, Segmented, Setting, Slider, Switch } from "../components/ui";

export function AiSection({ s, push }: { s: Settings; push: (n: Settings) => void }) {
  const eng = useEngine(); const [busy, setBusy] = useState(false); const [err, setErr] = useState<string | null>(null);
  const set = (p: Partial<AiSettings>) => push({ ...s, ai: { ...s.ai, ...p } });
  const st = eng?.status; const ready = !!st?.loaded;
  const reload = async () => { if (!s.ai.model_id) return; setBusy(true); setErr(null); try { await ai.load(s.ai.model_id); } catch (e) { setErr(String(e)); } finally { setBusy(false); } };
  return (
    <div className="stack">
      <Card title="Local model" hint="Runs inside Veyra itself, on this computer. Nothing is sent anywhere.">
        <div className="statusrow">
          <span className={`pulse ${ready ? "ok" : eng?.loading ? "busy" : "off"}`} aria-hidden="true" />
          <div><b>{ready ? "Ready" : eng?.loading ? "Loading the model…" : "No model in memory"}</b>
            <p className="hint">{ready ? `${st?.model} — stays in memory while the chat window is closed` : eng?.selected ? `Selected: ${eng.selected}` : "Choose one in Models."}</p></div>
          <span className="grow" />{ready ? <Button busy={busy} onClick={() => { setBusy(true); void ai.unload().finally(() => setBusy(false)); }}>Unload</Button> : <Button variant="primary" busy={busy || !!eng?.loading} disabled={!s.ai.model_id} onClick={reload}>Load</Button>}
        </div>
        {eng?.error && <Notice kind="error">{eng.error.split("\n").map((l, i) => <p key={i}>{l}</p>)}</Notice>}{err && <Notice kind="error">{err}</Notice>}
      </Card>
      <Card title="Behaviour">
        <Setting title="Load the model when Veyra starts" hint="Once per session. Closing the chat window never unloads it."><Switch label="Load at start" checked={s.ai.auto_load} onChange={(v) => set({ auto_load: v })} /></Setting>
        <Setting title="Answer style" hint="“Fast” answers directly. “Reasoning” lets the model think first: noticeably slower, sometimes better on hard problems. The thinking itself is never shown."><Segmented label="Answer style" value={s.ai.thinking} onChange={(v) => set({ thinking: v })} options={[["off", "Fast"], ["on", "Reasoning"]]} /></Setting>
        <Setting title="Free memory when idle" hint="Unloads the model after a period without use; the next question reloads it."><select aria-label="Idle unload" value={s.ai.idle_unload_minutes ?? 0} onChange={(e) => set({ idle_unload_minutes: Number(e.target.value) || null })}><option value={0}>Never</option><option value={5}>After 5 minutes</option><option value={15}>After 15 minutes</option><option value={30}>After 30 minutes</option><option value={60}>After 1 hour</option></select></Setting>
        <Setting title="Longest answer" hint="Maximum length of one reply."><Slider label="Longest answer" value={s.ai.max_reply_tokens} min={128} max={4096} step={128} onChange={(v) => set({ max_reply_tokens: v })} format={(v) => `${v} tokens`} /></Setting>
      </Card>
    </div>
  );
}
