import { useState, type ReactNode } from "react";
import { parseBlocks, type Inline } from "../lib/markdown";

const inl = (c: Inline[]): ReactNode[] => c.map((n, i) => n.t === "text" ? n.v : n.t === "code" ? <code key={i}>{n.v}</code> : n.t === "b" ? <strong key={i}>{inl(n.c)}</strong> : <em key={i}>{inl(n.c)}</em>);

export function CopyButton({ text, label = "Copy" }: { text: string; label?: string }) {
  const [done, setDone] = useState(false);
  const copy = async () => { try { await navigator.clipboard.writeText(text); setDone(true); setTimeout(() => setDone(false), 1400); } catch { /* clipboard unavailable: nothing to show */ } };
  return <button className="copy" onClick={copy} aria-label={label}>{done ? "Copied" : label}</button>;
}

export function Md({ text }: { text: string }) {
  return <div className="md">{parseBlocks(text).map((b, i) => {
    switch (b.t) {
      case "code": return <div className="codeblock" key={i}><div className="codehead"><span>{b.lang || "code"}</span><CopyButton text={b.v} label="Copy code" /></div><pre><code>{b.v}</code></pre></div>;
      case "h": return <p key={i} className={`h h${b.level}`}>{inl(b.c)}</p>;
      case "ul": return <ul key={i}>{b.items.map((x, j) => <li key={j}>{inl(x)}</li>)}</ul>;
      case "ol": return <ol key={i}>{b.items.map((x, j) => <li key={j}>{inl(x)}</li>)}</ol>;
      case "quote": return <blockquote key={i}>{inl(b.c)}</blockquote>;
      default: return <p key={i}>{inl(b.c)}</p>;
    }
  })}</div>;
}
