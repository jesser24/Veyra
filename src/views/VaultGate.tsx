import { useState } from "react";
import { vault } from "../lib/ai";

/** Master password: created on first use, asked at each start. The recovery key is shown once and must be confirmed as saved. */
export function VaultGate({ exists, onOpen }: { exists: boolean; onOpen: () => void }) {
  const [pw, setPw] = useState(""); const [pw2, setPw2] = useState(""); const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false); const [recovery, setRecovery] = useState<string | null>(null); const [useRecovery, setUseRecovery] = useState(false); const [saved, setSaved] = useState(false);
  const run = async (f: () => Promise<void>) => { setBusy(true); setErr(null); try { await f(); } catch (e) { setErr(String(e)); } finally { setBusy(false); } };
  if (recovery) return (
    <div className="gate"><h2>Your recovery key</h2>
      <p>If you forget your master password, this key is the <b>only</b> way to open your conversations. Veyra cannot recover it for you and keeps no copy. Write it down or store it in a password manager.</p>
      <pre className="recovery" aria-label="Recovery key">{recovery}</pre>
      <label className="check"><input type="checkbox" checked={saved} onChange={(e) => setSaved(e.target.checked)} /> I have saved my recovery key</label>
      <button className="primary" disabled={!saved} onClick={onOpen}>Continue</button></div>);
  if (exists) return (
    <form className="gate" onSubmit={(e) => { e.preventDefault(); void run(() => useRecovery ? vault.unlockRecovery(pw).then(onOpen) : vault.unlock(pw).then(onOpen)); }}>
      <h2>Unlock Veyra</h2><p>Your conversations are encrypted on this computer.</p>
      <input type="password" autoFocus value={pw} onChange={(e) => setPw(e.target.value)} placeholder={useRecovery ? "Recovery key" : "Master password"} aria-label={useRecovery ? "Recovery key" : "Master password"} />
      <button className="primary" disabled={busy || !pw}>{busy ? "Unlocking…" : "Unlock"}</button>
      <button type="button" className="link" onClick={() => { setUseRecovery(!useRecovery); setPw(""); setErr(null); }}>{useRecovery ? "Use my password" : "I forgot my password"}</button>
      {err && <p className="err" role="alert">{err}</p>}</form>);
  return (
    <form className="gate" onSubmit={(e) => { e.preventDefault(); if (pw !== pw2) { setErr("The two passwords differ."); return; } void run(async () => setRecovery(await vault.create(pw))); }}>
      <h2>Create your master password</h2><p>It encrypts your conversations and attachments on this computer. It never leaves it. Minimum 8 characters.</p>
      <input type="password" autoFocus value={pw} onChange={(e) => setPw(e.target.value)} placeholder="Master password" aria-label="Master password" />
      <input type="password" value={pw2} onChange={(e) => setPw2(e.target.value)} placeholder="Repeat the password" aria-label="Repeat the master password" />
      <button className="primary" disabled={busy || pw.length < 8}>{busy ? "Creating…" : "Create"}</button>
      {err && <p className="err" role="alert">{err}</p>}</form>);
}
