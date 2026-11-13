import { useEffect, useState } from "react";
import { fmtBytes, net, type NetEntry, type NetSummary } from "../lib/ai";

const PURPOSE: Record<string, string> = { ModelDownload: "Model download", WebSearch: "Web search", Update: "Update", OtherService: "Other service", LocalEngine: "Local engine (this computer only)" };

export function NetworkView() {
  const [s, setS] = useState<NetSummary | null>(null); const [e, setE] = useState<NetEntry[]>([]);
  const load = () => Promise.all([net.summary(), net.entries()]).then(([a, b]) => { setS(a); setE(b); }).catch(() => {});
  useEffect(() => { void load(); const iv = setInterval(() => { if (!document.hidden) void load(); }, 2000); return () => clearInterval(iv); }, []);
  if (!s) return null;
  const ext = e.filter((x) => !x.local);
  return (
    <div className="netv">
      <section className="note2"><b>What this shows.</b> Every connection Veyra itself opens goes through one gateway, which records host, time, size and result (never the path, content or headers). Connections made by other programs, the operating system or the webview engine are <b>not</b> visible here; use a system tool (Resource Monitor, Little Snitch, Wireshark) to audit those.</section>
      <h2>Measured by Veyra</h2>
      <dl className="grid">
        <dt>External requests</dt><dd>{s.external_requests}</dd><dt>Local engine requests (loopback)</dt><dd>{s.local_requests}</dd>
        <dt>In progress</dt><dd>{s.in_flight}</dd><dt>Sent / received</dt><dd>{fmtBytes(s.bytes_sent)} / {fmtBytes(s.bytes_received)}</dd>
        <dt>Average latency</dt><dd>{s.avg_latency_ms != null ? `${s.avg_latency_ms} ms` : "—"}</dd>
        <dt>Errors</dt><dd>{s.errors} (DNS {s.dns_errors}, TLS {s.tls_errors}, proxy {s.proxy_errors}, timeouts {s.timeouts})</dd>
        <dt>Hosts contacted</dt><dd>{s.hosts.filter((h) => h !== "127.0.0.1").join(", ") || "none"}</dd>
      </dl>
      <p className="meta">{ext.length === 0 ? "No external connection has been made since Veyra started." : "External connections happen only after you press Download (or run a web search)."} Error classes are inferred from the HTTP client's messages and may be approximate.</p>
      <h2>Recent requests</h2>
      <table className="log"><thead><tr><th>Time</th><th>Purpose</th><th>Host</th><th>Status</th><th>Down</th></tr></thead>
        <tbody>{[...e].reverse().slice(0, 40).map((x) => <tr key={x.id} className={x.local ? "loc" : ""}><td>{new Date(x.started_ms).toLocaleTimeString()}</td><td>{PURPOSE[x.purpose] ?? x.purpose}</td><td>{x.host}</td><td>{x.in_flight ? "…" : x.error ?? x.status}</td><td>{fmtBytes(x.bytes_received)}</td></tr>)}</tbody></table>
      <div className="btns"><button onClick={() => net.clear().then(load)}>Clear this log</button></div>
    </div>
  );
}
