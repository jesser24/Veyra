import { useRef, useState } from "react";
import { security } from "../lib/ai";
import { useSecurity } from "../lib/hooks";
import { Button, Card, CopyButton, Notice, Skeleton } from "../components/ui";

export function SecuritySection() {
  const [st, refresh] = useSecurity(); const [pass, setPass] = useState(""); const [out, setOut] = useState<string | null>(null); const [err, setErr] = useState<string | null>(null); const [busy, setBusy] = useState(false);
  const [ipass, setIpass] = useState(""); const [bundle, setBundle] = useState<string | null>(null); const [iname, setIname] = useState(""); const [iok, setIok] = useState(false); const file = useRef<HTMLInputElement>(null);
  if (!st) return <Skeleton lines={4} />;
  const exp = async () => { setBusy(true); setErr(null); setOut(null); try { setOut(await security.exportKey(pass)); setPass(""); } catch (e) { setErr(String(e)); } finally { setBusy(false); } };
  const imp = async () => { if (!bundle) return; setBusy(true); setErr(null); try { await invoke_import(bundle, ipass); setIok(true); setIpass(""); refresh(); } catch (e) { setErr(String(e)); } finally { setBusy(false); } };
  return (
    <div className="stack">
      {st.error ? <Notice kind="error">{st.error}</Notice> : <Notice kind={st.key_kind === "os" ? "ok" : "warn"}>{st.ready ? "Your conversations and files are encrypted on this computer." : "Storage is starting…"}</Notice>}
      <Card title="How your data is protected">
        <dl className="facts"><dt>Encryption</dt><dd>SQLCipher (AES-256) for conversations, XChaCha20-Poly1305 for attached files.</dd>
          <dt>Key</dt><dd>Random, created on first start and kept by <b>{st.key_protection}</b>. You never type a password to use Veyra.</dd>
          <dt>Data folder</dt><dd><code>{st.data_dir}</code></dd></dl>
        <p className="hint">What this protects: someone who copies your data files (a stolen disk, a cloud-synced folder, a backup) cannot read them. What it does not protect: anyone who can sign in to this operating-system account, or malware running as you, can use the key like Veyra does. Use a strong account password and disk encryption (BitLocker / FileVault).</p>
      </Card>
      <Card title="Installation key" hint="Asked once, when Veyra is first configured.">
        <p className="hint">The installation key only confirms the first configuration. It is <b>not</b> an encryption key, does not protect your data, and is not a licensing mechanism: any key shipped inside an application can be extracted from it. Your data's protection is entirely the system-protected key above.</p>
      </Card>
      <Card title="Move to another computer" hint="Backups can only be read with this computer's data key, which stays in the system credential store.">
        <p className="hint">To restore a backup elsewhere, export the key with a passphrase (10+ characters) and keep that file apart from the backup. Anyone holding both the file and the passphrase can read your backups.</p>
        <div className="inline wrap"><input className="field" type="password" autoComplete="new-password" placeholder="Passphrase for the key export" value={pass} onChange={(e) => setPass(e.target.value)} aria-label="Export passphrase" /><Button busy={busy} disabled={pass.length < 10} onClick={exp}>Export key</Button></div>
        {out && <Notice kind="ok" action={<CopyButton text={out} label="Copy path" />}>Saved to <code>{out}</code></Notice>}
        <hr />
        <p className="hint"><b>On the new computer</b> (before using it): import the exported key, then restore the backup in Backups. Only possible while that Veyra holds no conversations of its own.</p>
        <div className="inline wrap"><input ref={file} type="file" accept=".json" hidden onChange={async (e) => { const f = e.target.files?.[0]; e.target.value = ""; if (f) { setBundle(await f.text()); setIname(f.name); setIok(false); } }} />
          <Button onClick={() => file.current?.click()}>{iname || "Choose the key export…"}</Button><input className="field" type="password" autoComplete="off" placeholder="Its passphrase" value={ipass} onChange={(e) => setIpass(e.target.value)} aria-label="Import passphrase" /><Button variant="primary" busy={busy} disabled={!bundle || ipass.length < 10} onClick={imp}>Import key</Button></div>
        {iok && <Notice kind="ok">Key imported. Now restore your backup in the Backups section.</Notice>}
      </Card>
      {err && <Notice kind="error">{err}</Notice>}
    </div>
  );
}
import { invoke } from "@tauri-apps/api/core";
const invoke_import = (bundle: string, passphrase: string) => invoke<void>("key_import", { bundle, passphrase });
