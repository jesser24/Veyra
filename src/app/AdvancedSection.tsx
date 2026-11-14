import { useState } from "react";
import { ai, fmtMs, fmtRate, type Perf } from "../lib/ai";
import { api } from "../lib/api";
import { useEngine } from "../lib/hooks";
import type { AiSettings, Settings } from "../lib/types";
import { Button, Card, Notice, Segmented, Setting, Slider, Switch } from "../components/ui";

/** Engine tuning and the only place where performance numbers are shown. Changes apply at the next model load. */
export function AdvancedSection({ s, push }: { s: Settings; push: (n: Settings) => void }) {
  const eng = useEngine(); const [test, setTest] = useState<{ text: string; perf: Perf; rss_mib: number | null } | null>(null); const [busy, setBusy] = useState<string | null>(null); const [err, setErr] = useState<string | null>(null);
  const set = (p: Partial<AiSettings>) => push({ ...s, ai: { ...s.ai, ...p } }); const st = eng?.status; const ready = !!st?.loaded;
  const run = async (n: string, f: () => Promise<void>) => { setBusy(n); setErr(null); try { await f(); } catch (e) { setErr(String(e)); } finally { setBusy(null); } };
  return (
    <div className="stack">
      {err && <Notice kind="error">{err}</Notice>}
      <Card title="Engine" hint="llama.cpp, linked into Veyra's own process: no server, no helper program.">
        <dl className="facts"><dt>Library</dt><dd>{st?.llama ?? "—"}</dd><dt>Model</dt><dd>{st?.model ?? "none loaded"}</dd>
          {ready && <><dt>Context</dt><dd>{st?.ctx} tokens</dd><dt>Threads</dt><dd>{st?.threads}</dd><dt>Memory used by Veyra</dt><dd>{st?.rss_mib != null ? `${st.rss_mib} MB` : "—"}</dd><dt>Load time</dt><dd>{fmtMs(st?.load_ms)}</dd></>}</dl>
      </Card>
      <Card title="Tuning" hint="Automatic values are chosen from this computer's hardware. These apply the next time the model loads.">
        <Setting title="Context size" hint="Larger = longer conversations remembered, more memory, slower on long prompts."><select aria-label="Context size" value={s.ai.ctx ?? 0} onChange={(e) => set({ ctx: Number(e.target.value) || null })}><option value={0}>Automatic</option>{[2048, 4096, 8192, 16384, 32768].map((n) => <option key={n} value={n}>{n}</option>)}</select></Setting>
        <Setting title="Threads" hint="More is not always faster; automatic uses the physical cores."><select aria-label="Threads" value={s.ai.threads ?? 0} onChange={(e) => set({ threads: Number(e.target.value) || null })}><option value={0}>Automatic</option>{Array.from({ length: 32 }, (_, i) => i + 1).map((n) => <option key={n} value={n}>{n}</option>)}</select></Setting>
        <Setting title="Prompt batch size"><Slider label="Batch size" min={64} max={2048} step={64} value={s.ai.batch} onChange={(v) => set({ batch: v })} format={(v) => String(v)} /></Setting>
        <Setting title="Flash attention"><Segmented label="Flash attention" value={s.ai.flash_attention} onChange={(v) => set({ flash_attention: v })} options={[["auto", "Auto"], ["on", "On"], ["off", "Off"]]} /></Setting>
        <Setting title="Compress the memory cache (8-bit)" hint="Saves memory on long contexts at a tiny quality cost."><Switch label="KV cache 8-bit" checked={s.ai.kv_cache_q8} onChange={(v) => set({ kv_cache_q8: v })} /></Setting>
        <div className="actions"><Button variant="primary" disabled={!s.ai.model_id || !!eng?.loading} busy={busy === "reload"} onClick={() => run("reload", () => ai.load(s.ai.model_id!))}>Reload the model with these settings</Button></div>
      </Card>
      <Card title="Measure" hint="Runs one real generation now and reports what the engine measured on this computer.">
        <Button disabled={!ready} busy={busy === "test"} onClick={() => run("test", async () => setTest(await ai.test()))}>Run a test generation</Button>
        {test && <div className="testres"><p>“{test.text}”</p><dl className="facts"><dt>First token</dt><dd>{fmtMs(test.perf.first_token_ms)}</dd><dt>Prompt processing</dt><dd>{fmtRate(test.perf.prompt_tokens_per_s)}</dd><dt>Generation</dt><dd>{fmtRate(test.perf.generated_tokens_per_s)}</dd><dt>Total</dt><dd>{fmtMs(test.perf.wall_ms)}</dd><dt>Memory</dt><dd>{test.rss_mib ?? "?"} MB</dd></dl></div>}
        <Setting title="Show performance after each answer" hint="Adds a line under replies in the chat. Off by default."><Switch label="Show performance" checked={s.ai.show_performance} onChange={(v) => set({ show_performance: v })} /></Setting>
      </Card>
      <Card title="Veyra"><div className="actions"><Button variant="danger" onClick={() => void api.quit()}>Quit Veyra</Button></div></Card>
    </div>
  );
}
