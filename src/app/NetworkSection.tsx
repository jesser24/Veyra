import { useEffect, useState } from "react";
import { fmtBytes, net, type NetEntry, type NetSummary } from "../lib/ai";
import { Button, Card, Notice, Skeleton } from "../components/ui";

const PURPOSE: Record<string, string> = { ModelDownload: "Model download", WebSearch: "Web search", Update: "Update", OtherService: "Other service", LocalEngine: "Local engine" };
export function NetworkSection() {
  const [s, setS] = useState<NetSummary | null>(null); const [e, setE] = useState<NetEntry[]>([]);
  const load = () => Promise.all([net.summary(), net.entries()]).then(([a, b]) => { setS(a); setE(b); }).catch(() => {});
  useEffect(() => { void load(); const iv = setInterval(() => { if (!document.hidden) void load(); }, 2000); return () => clearInterval(iv); }, []);
  if (!s) return <Skeleton lines={4} />;
  const ext = e.filter((x) => !x.local);
  return (
    <div className="stack">
      <Notice kind="ok">No telemetry, no analytics, no tracking. {ext.length === 0 ? "Veyra has made no connection to the Internet since it started." : "The only connections are the ones you asked for, listed below."}</Notice>
      <div className="tiles">
        <div className="tile"><small>Internet requests</small><b>{s.external_requests}</b></div>
        <div className="tile"><small>Local server requests</small><b>{s.local_requests}</b><em>the engine runs in-process</em></div>
        <div className="tile"><small>Received</small><b>{fmtBytes(s.bytes_received)}</b></div>
        <div className="tile"><small>Sent</small><b>{fmtBytes(s.bytes_sent)}</b></div>
      </div>
      <Card title="Hosts contacted" hint="Veyra's own gateway records host, time, size and result. Never the path, the content or any header.">
        <p>{s.hosts.filter((h) => h !== "127.0.0.1").join(", ") || "None."}</p>
        <p className="hint">Errors: {s.errors} (DNS {s.dns_errors}, TLS {s.tls_errors}, proxy {s.proxy_errors}, timeouts {s.timeouts}) — inferred from the HTTP client's messages, so approximate.</p>
      </Card>
      <Card title="Recent requests" actions={<Button variant="quiet" onClick={() => net.clear().then(load)}>Clear log</Button>}>
        {e.length === 0 ? <p className="hint">Nothing yet.</p> : <div className="tablewrap"><table className="log"><thead><tr><th>Time</th><th>Purpose</th><th>Host</th><th>Result</th><th>Received</th></tr></thead>
          <tbody>{[...e].reverse().slice(0, 50).map((x) => <tr key={x.id}><td>{new Date(x.started_ms).toLocaleTimeString()}</td><td>{PURPOSE[x.purpose] ?? x.purpose}</td><td>{x.host}</td><td>{x.in_flight ? "…" : x.error ?? x.status}</td><td>{fmtBytes(x.bytes_received)}</td></tr>)}</tbody></table></div>}
      </Card>
      <p className="hint">Limit of this view: connections made by other programs, the operating system or the web view engine are not visible here. Use a system tool (Resource Monitor, Little Snitch, Wireshark) to audit those.</p>
    </div>
  );
}
