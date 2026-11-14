import { useState } from "react";
import { security } from "../lib/ai";
import { Button, Notice } from "../components/ui";

/** One-time installation step. The key confirms first configuration only; it protects nothing (see Security). */
export function Setup({ compact }: { compact?: boolean }) {
  const [key, setKey] = useState(""); const [err, setErr] = useState<string | null>(null); const [busy, setBusy] = useState(false);
  const go = async () => { setBusy(true); setErr(null); try { await security.setup(key); } catch (e) { setErr(String(e)); } finally { setBusy(false); } };
  return (
    <form className={`setup ${compact ? "compact" : ""}`} onSubmit={(e) => { e.preventDefault(); void go(); }}>
      <img src="/veyra-mark.png" alt="" width={56} height={56} draggable={false} />
      <h1>Welcome to Veyra</h1>
      <p>Enter your installation key to finish setting up. You will not be asked again, and no password is needed to use Veyra.</p>
      <input className="field" type="password" autoFocus autoComplete="off" placeholder="Installation key" aria-label="Installation key" value={key} onChange={(e) => setKey(e.target.value)} />
      {err && <Notice kind="error">{err}</Notice>}
      <Button variant="primary" type="submit" busy={busy} disabled={!key.trim()}>Continue</Button>
      <small>This key only confirms the installation. It does not encrypt or protect your data.</small>
    </form>
  );
}
