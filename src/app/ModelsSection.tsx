import { useCallback, useEffect, useRef, useState } from "react";
import { ai, fmtBytes, type DlEvent, type ModelView, type Overview } from "../lib/ai";
import { useEngine, useListen } from "../lib/hooks";
import { Button, Card, Icon, Notice, Skeleton } from "../components/ui";

const STATE_LABEL: Record<string, string> = { available: "Not downloaded", partial: "Download interrupted", downloaded: "Downloaded — not verified", verified: "Verified", loaded: "Loaded" };

/** Honest lifecycle: not downloaded → downloaded (not usable) → verified (SHA-256 matches the catalog) → loaded. */
export function ModelsSection() {
  const eng = useEngine(); const [o, setO] = useState<Overview | null>(null); const [err, setErr] = useState<string | null>(null); const [dl, setDl] = useState<DlEvent | null>(null); const [busy, setBusy] = useState<string | null>(null);
  const t0 = useRef<{ t: number; b: number } | null>(null); const [speed, setSpeed] = useState<number | null>(null);
  const refresh = useCallback(() => { ai.overview().then(setO).catch((e) => setErr(String(e))); }, []);
  useEffect(refresh, [refresh]);
  useEffect(() => { refresh(); }, [eng?.status.loaded, eng?.loading, eng?.downloading, refresh]);
  useListen(() => ai.onDownload((e) => {
    setDl(e);
    if (e.phase === "downloading") { const now = performance.now(); if (!t0.current) t0.current = { t: now, b: e.done }; else if (now - t0.current.t > 1500) { setSpeed(((e.done - t0.current.b) / (now - t0.current.t)) * 1000); t0.current = { t: now, b: e.done }; } }
    if (["verified", "cancelled", "error"].includes(e.phase)) { t0.current = null; setSpeed(null); if (e.error) setErr(e.error); refresh(); }
  }), [refresh]);
  const act = async (name: string, f: () => Promise<unknown>) => { setBusy(name); setErr(null); try { await f(); } catch (e) { setErr(String(e)); } finally { setBusy(null); refresh(); } };
  if (!o) return <Skeleton lines={5} />;
  const hw = o.hardware;
  const card = (m: ModelView) => {
    const dling = dl && dl.id === m.id && ["downloading", "verifying"].includes(dl.phase) && o.engine.downloading; const pct = dling && dl.total ? Math.min(100, (dl.done / dl.total) * 100) : null; const state = m.loaded ? "loaded" : m.state;
    const loadingThis = busy === `load-${m.id}` || (eng?.loading && eng.selected === m.id);
    return (
      <article key={m.id} className={`model ${m.loaded ? "is-loaded" : ""}`}>
        <div className="m-head"><div><h4>{m.name}</h4>{m.status === "verified" ? <p className="hint">{m.size_gb.toFixed(2)} GB · {m.quant} · needs about {m.min_ram_gb} GB of RAM · {m.license}</p> : <p className="hint">{m.params_b} B · {m.license}</p>}</div>
          <div className="m-tags">{m.recommended && <span className="tag accent">Recommended</span>}{m.status === "candidate" && <span className="tag warn">Not available yet</span>}{m.status === "verified" && <span className={`tag state-${state}`}>{STATE_LABEL[state]}</span>}</div></div>
        <p className="m-notes">{m.notes}</p>
        {m.status === "verified" && !m.fits && <Notice kind="warn">This computer has too little memory for this model.</Notice>}
        {dling && <div className="progress" role="progressbar" aria-valuenow={pct ?? undefined} aria-valuemin={0} aria-valuemax={100} aria-label={dl.phase}><i style={{ width: `${pct ?? 100}%` }} /><span>{dl.phase === "verifying" ? "Checking integrity (SHA-256)…" : `${fmtBytes(dl.done)}${dl.total ? ` of ${fmtBytes(dl.total)}` : ""}${speed ? ` · ${fmtBytes(speed)}/s` : ""}`}</span></div>}
        {m.status === "verified" && (
          <div className="m-actions">
            {(m.state === "available" || m.state === "partial") && !dling && <Button variant="primary" icon="down" disabled={!!busy || o.engine.downloading || !m.fits} onClick={() => act("dl", () => ai.download(m.id))}>{m.state === "partial" ? `Resume (${fmtBytes(m.bytes_on_disk)} kept)` : "Download"}</Button>}
            {dling && dl.phase === "downloading" && <Button onClick={() => void ai.cancelDownload()}>Cancel</Button>}
            {m.state === "downloaded" && <Button variant="primary" busy={busy === "verify"} onClick={() => act("verify", async () => { if (!(await ai.verify(m.id))) throw new Error("The SHA-256 does not match the catalog: this is not the official file. Delete it and download it again."); })}>Verify integrity</Button>}
            {m.state === "verified" && !m.loaded && <Button variant="primary" busy={!!loadingThis} disabled={!!busy || !!eng?.loading} onClick={() => act(`load-${m.id}`, () => ai.load(m.id))}>{loadingThis ? "Loading…" : eng?.selected === m.id ? "Load" : "Use this model"}</Button>}
            {m.loaded && <Button busy={busy === "unload"} onClick={() => act("unload", () => ai.unload())}>Unload from memory</Button>}
            {m.state !== "available" && !m.loaded && !dling && <Button variant="quiet" disabled={!!busy} onClick={() => act("del", () => ai.deleteModel(m.id))}>Delete file</Button>}
          </div>)}
        {m.sha256 && <p className="hash" title="SHA-256 published by the official Hugging Face repository; checked after every download">SHA-256 {m.sha256.slice(0, 20)}…</p>}
      </article>);
  };
  return (
    <div className="stack">
      {err && <Notice kind="error" action={<Button variant="quiet" onClick={() => setErr(null)}>Dismiss</Button>}>{err}</Notice>}
      <Card title="This computer" hint="Measured now, locally.">
        <dl className="facts">
          <dt>Processor</dt><dd>{hw.cpu_brand || hw.arch} — {hw.physical_cores} cores / {hw.logical_cores} threads{hw.avx2 && " · AVX2"}{hw.apple_silicon && " · Apple Silicon"}</dd>
          <dt>Memory</dt><dd>{(hw.total_ram_mib / 1024).toFixed(1)} GB, {(hw.available_ram_mib / 1024).toFixed(1)} GB free</dd>
          <dt>Free disk</dt><dd>{o.free_disk_bytes != null ? fmtBytes(o.free_disk_bytes) : "unknown"}</dd>
        </dl>
        {o.recommendation && <p className="recommend"><Icon name="info" size={15} />{o.recommendation.reason}{o.recommendation.warnings.map((w) => <b key={w}> {w}</b>)}</p>}
      </Card>
      <div className="models">{o.models.map(card)}</div>
      <p className="hint">Models are downloaded only when you press Download, from the official Hugging Face repositories, and every file is checked against its published SHA-256 before it can be loaded. Folder: <code>{o.models_dir}</code></p>
    </div>
  );
}
