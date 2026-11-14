import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { fmtBytes, security, type BackupInfo } from "../lib/ai";
import { Button, Card, Empty, Notice, Skeleton } from "../components/ui";

export function BackupsSection() {
  const [list, setList] = useState<BackupInfo[] | null>(null); const [busy, setBusy] = useState<string | null>(null); const [msg, setMsg] = useState<{ k: "ok" | "error"; t: string } | null>(null); const [confirm, setConfirm] = useState<string | null>(null);
  const refresh = useCallback(() => { security.backupList().then(setList).catch(() => setList([])); }, []); useEffect(refresh, [refresh]);
  const run = async (name: string, f: () => Promise<string | void>, ok: (r: string | void) => string) => { setBusy(name); setMsg(null); try { const r = await f(); setMsg({ k: "ok", t: ok(r) }); } catch (e) { setMsg({ k: "error", t: String(e) }); } finally { setBusy(null); setConfirm(null); refresh(); } };
  const when = (t: number) => new Date(t * 1000).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });
  return (
    <div className="stack">
      {msg && <Notice kind={msg.k}>{msg.t}</Notice>}
      <Card title="Backups" hint="A consistent, checksummed copy of your conversations and attached files. Kept in Veyra's data folder; old ones are pruned automatically (90 days, 20 copies, 2 GB)."
        actions={<Button variant="primary" icon="save" busy={busy === "create"} onClick={() => run("create", () => security.backupCreate(), (p) => `Backup created: ${p}`)}>Back up now</Button>}>
        {list === null && <Skeleton lines={3} />}
        {list && list.length === 0 && <Empty icon="save" title="No backup yet">Press “Back up now”. Nothing is backed up automatically.</Empty>}
        <ul className="rows">{list?.map((b) => (
          <li key={b.name}><div className="row-main"><b>{when(b.modified)}</b><small>{b.name} · {fmtBytes(b.bytes)}</small></div>
            <div className="row-acts">
              <Button busy={busy === `v${b.name}`} onClick={() => run(`v${b.name}`, () => invoke<void>("backup_verify", { name: b.name }), () => "Backup verified: every file matches its checksum.")}>Verify</Button>
              {confirm === b.name ? <><Button variant="danger" busy={busy === `r${b.name}`} onClick={() => run(`r${b.name}`, () => invoke<string>("backup_restore", { name: b.name }), (p) => `Restored. Your previous data was kept in ${p}`)}>Replace current data</Button><Button onClick={() => setConfirm(null)}>Cancel</Button></> : <Button onClick={() => setConfirm(b.name)}>Restore…</Button>}</div></li>))}</ul>
      </Card>
      <p className="hint">Restoring replaces the current conversations with the backup's; the current data is never deleted, it is moved aside in the data folder. A backup made on another computer needs that computer's key first (Security → Move to another computer), and its folder copied into this computer's <code>backups</code> folder.</p>
    </div>
  );
}
