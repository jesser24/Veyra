import type { ReactNode } from "react";
import { parseBlocks, type Inline } from "../lib/markdown";
import { CopyButton } from "./ui";

const inl = (c: Inline[]): ReactNode[] => c.map((n, i) => n.t === "text" ? n.v : n.t === "code" ? <code key={i}>{n.v}</code> : n.t === "b" ? <strong key={i}>{inl(n.c)}</strong> : <em key={i}>{inl(n.c)}</em>);

/** Renders the Markdown tree with React elements only (never innerHTML). Wide code and tables scroll inside their own box instead of widening the window. */
export function Md({ text }: { text: string }) {
  return <div className="md">{parseBlocks(text).map((b, i) => {
    switch (b.t) {
      case "code": return <div className="codeblock" key={i}><div className="codehead"><span>{b.lang || "code"}</span>{!b.open && <CopyButton text={b.v} label="Copy" />}</div><pre tabIndex={0}><code>{b.v}</code></pre></div>;
      case "h": return <p key={i} className={`h h${b.level}`}>{inl(b.c)}</p>;
      case "ul": return <ul key={i}>{b.items.map((x, j) => <li key={j}>{inl(x)}</li>)}</ul>;
      case "ol": return <ol key={i}>{b.items.map((x, j) => <li key={j}>{inl(x)}</li>)}</ol>;
      case "quote": return <blockquote key={i}>{inl(b.c)}</blockquote>;
      case "hr": return <hr key={i} />;
      case "table": return <div className="tablewrap" key={i} tabIndex={0}><table><thead><tr>{b.head.map((h, j) => <th key={j} style={{ textAlign: b.align[j] ?? undefined }}>{inl(h)}</th>)}</tr></thead>
        <tbody>{b.rows.map((r, j) => <tr key={j}>{r.map((c, k) => <td key={k} style={{ textAlign: b.align[k] ?? undefined }}>{inl(c)}</td>)}</tr>)}</tbody></table></div>;
      default: return <p key={i}>{inl(b.c)}</p>;
    }
  })}</div>;
}
