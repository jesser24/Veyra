import { useCallback, useEffect, useRef, useState } from "react";
import { ai, fmtBytes, fmtMs, fmtRate, type DlEvent, type ModelView, type Overview, type Perf } from "../lib/ai";

/** Honest state machine shown to the user: available → partial → downloaded (NOT usable) → verified (SHA-256 OK) → loaded → engine ready. */
const STATE_LABEL: Record<string, string> = { available: "Available", partial: "Download interrupted", downloaded: "Downloaded, not verified", verified: "Verified", loaded: "Loaded" };

export function AiManager({ onChanged }: { onChanged: () => void }) {
  const [o, setO] = useState<Overview | null>(null); const [err, setErr] = useState<string | null>(null);
  const [dl, setDl] = useState<DlEvent | null>(null); const [busy, setBusy] = useState<string | null>(null);
  const [test, setTest] = useState<{ text: string; perf: Perf; rss_mib: number | null } | null>(null);
  const t0 = useRef<{ t: number; b: number } | null>(null); const [speed, setSpeed] = useState<number | null>(null);
  const refresh = useCallback(() => ai.overview().then(setO).catch((e) => setErr(String(e))), []);
  useEffect(() => { void refresh(); const iv = setInterval(() => { if (!document.hidden) void ai.status().then(() => refresh()); }, 4000); return () => clearInterval(iv); }, [refresh]);
  useEffect(() => {
    const a = ai.onDownload((e) => {
      setDl(e);
      if (e.phase === "downloading") { const now = performance.now(); if (!t0.current) t0.current = { t: now, b: e.done }; else if (now - t0.current.t > 1500) { setSpeed(((e.done - t0.current.b) / (now - t0.current.t)) * 1000); t0.current = { t: now, b: e.done }; } }
      if (["verified", "cancelled", "error"].includes(e.phase)) { t0.current = null; setSpeed(null); if (e.error) setErr(e.error); void refresh(); }
    });
    const b = ai.onEngine(() => { void refresh(); onChanged(); });
    return () => { void a.then((f) => f()); void b.then((f) => f()); };
  }, [refresh, onChanged]);

  const act = async (name: string, f: () => Promise<unknown>) => { setBusy(name); setErr(null); try { await f(); } catch (e) { setErr(String(e)); } finally { setBusy(null); void refresh(); onChanged(); } };
  if (!o) return <p className="muted pad">Reading hardware…</p>;
  const st = o.status; const ready = st.loaded;
  const row = (m: ModelView) => {
    const dling = dl && dl.id === m.id && ["downloading", "verifying"].includes(dl.phase) && o.downloading;
    const pct = dling && dl.total ? Math.min(100, (dl.done / dl.total) * 100) : null;
    const state = m.loaded ? "loaded" : m.state;
    return (
      <article key={m.id} className={`model ${m.status} ${m.recommended ? "rec" : ""}`}>
        <header><strong>{m.name}</strong>{m.recommended && <span className="badge">Recommended for this computer</span>}{m.status === "candidate" && <span className="badge warn">Candidate – not available</span>}</header>
        <p className="meta">{m.status === "verified" ? <>{m.size_gb.toFixed(2)} GB · {m.quant} · {m.license} · needs ≈ {m.min_ram_gb} GB RAM{!m.fits && <b className="err"> · too large for this computer</b>}</> : <>{m.params_b} B · {m.license}</>}</p>
        <p className="notes">{m.notes}</p>
        {m.status === "verified" && <>
          <p className="state"><span className={`dot ${state}`} />{STATE_LABEL[state]}{m.state === "partial" && ` (${fmtBytes(m.bytes_on_disk)} kept)`}</p>
          {m.sha256 && <p className="hash" title="SHA-256 published by the official Hugging Face repository, checked after download">SHA-256 {m.sha256.slice(0, 16)}…</p>}
          {dling && <div className="progress" role="progressbar" aria-valuenow={pct ?? undefined} aria-label={dl.phase}><i style={{ width: `${pct ?? 100}%` }} /><span>{dl.phase === "verifying" ? "Verifying integrity (SHA-256)…" : `${fmtBytes(dl.done)}${dl.total ? ` / ${fmtBytes(dl.total)}` : ""}${speed ? ` · ${fmtBytes(speed)}/s` : ""}`}</span></div>}
          <div className="btns">
            {(m.state === "available" || m.state === "partial") && !dling && <button className="primary" disabled={!!busy || o.downloading || !m.fits} onClick={() => act("dl", () => ai.download(m.id))}>{m.state === "partial" ? "Resume download" : "Download"}</button>}
            {dling && dl.phase === "downloading" && <button onClick={() => void ai.cancelDownload()}>Cancel</button>}
            {m.state === "downloaded" && <button className="primary" disabled={!!busy} onClick={() => act("verify", async () => { if (!(await ai.verify(m.id))) throw new Error("SHA-256 mismatch: the file is not the official one. Delete it and download again."); })}>Verify integrity</button>}
            {m.state === "verified" && !m.loaded && <button className="primary" disabled={!!busy || !o.engine_binary} onClick={() => act("load", () => ai.load(m.id))}>{busy === "load" ? "Loading…" : "Load"}</button>}
            {m.loaded && <button disabled={!!busy} onClick={() => act("unload", () => ai.unload())}>Unload</button>}
            {m.state !== "available" && !m.loaded && !dling && <button disabled={!!busy} onClick={() => act("del", () => ai.deleteModel(m.id))}>Delete file</button>}
          </div></>}
      </article>);
  };
  return (
    <div className="aim">
      <section className={`engine ${ready ? "ok" : ""}`} aria-live="polite">
        <h2>Engine</h2>
        <p><span className={`dot ${ready ? "loaded" : "available"}`} /><b>{ready ? "Ready" : "Not ready"}</b> — {ready ? <>{st.model} · context {st.ctx} · {st.threads} threads{st.rss_mib != null && <> · {st.rss_mib} MB RAM</>}</> : !o.engine_binary ? <span className="err">the inference engine (llama-server) is missing from this installation</span> : "no model loaded"}</p>
        {ready && <div className="btns"><button disabled={!!busy} onClick={() => act("test", async () => setTest(await ai.test()))}>{busy === "test" ? "Generating…" : "Test generation"}</button></div>}
        {test && <div className="testres"><p>“{test.text}”</p><p className="perf">{test.perf.generated_tokens ?? "?"} tokens · {fmtRate(test.perf.generated_tokens_per_s)} · prompt {fmtRate(test.perf.prompt_tokens_per_s)} · first token {fmtMs(test.perf.first_token_ms)} · load {fmtMs(test.perf.load_ms)} · {test.rss_mib ?? "?"} MB RAM — measured on this computer, now</p></div>}
        {!ready && o.last_perf && <p className="perf">Last generation: {fmtRate(o.last_perf.generated_tokens_per_s)}</p>}
      </section>
      <section><h2>This computer</h2>
        <p className="meta">{o.hardware.cpu_brand || o.hardware.arch} · {o.hardware.physical_cores} cores / {o.hardware.logical_cores} threads · {(o.hardware.total_ram_mib / 1024).toFixed(1)} GB RAM ({(o.hardware.available_ram_mib / 1024).toFixed(1)} GB free){o.hardware.avx2 && " · AVX2"}{o.hardware.apple_silicon && " · Apple Silicon"} · disk free {o.free_disk_bytes != null ? fmtBytes(o.free_disk_bytes) : "unknown"}</p>
        {o.recommendation && <p className="note2"><b>Recommendation.</b> {o.recommendation.reason}{o.recommendation.warnings.map((w) => <span key={w} className="err"> {w}</span>)}</p>}
        {!o.recommendation && <p className="err">No catalog model fits this computer's memory.</p>}
      </section>
      <section><h2>Models</h2>{o.models.map(row)}<p className="meta">Models are downloaded from the official Hugging Face repositories only when you press Download, and every file is checked against its published SHA-256 before it can be loaded. Folder: {o.models_dir}</p></section>
      {err && <p className="err" role="alert">{err}</p>}
    </div>
  );
}
