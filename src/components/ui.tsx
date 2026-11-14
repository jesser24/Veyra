import { useEffect, useRef, useState, type ButtonHTMLAttributes, type ReactNode } from "react";

const P: Record<string, string> = {
  history: "M3 12a9 9 0 1 0 3-6.7M3 4v4h4M12 7.5V12l3 2",
  gear: "M12 15.2a3.2 3.2 0 1 0 0-6.4 3.2 3.2 0 0 0 0 6.4ZM19.4 13.5a1.5 1.5 0 0 0 .3 1.7l.1.1a1.8 1.8 0 1 1-2.6 2.6l-.1-.1a1.5 1.5 0 0 0-1.7-.3 1.5 1.5 0 0 0-.9 1.4V19a1.8 1.8 0 1 1-3.6 0v-.1a1.5 1.5 0 0 0-1-1.4 1.5 1.5 0 0 0-1.7.3l-.1.1a1.8 1.8 0 1 1-2.6-2.6l.1-.1a1.5 1.5 0 0 0 .3-1.7 1.5 1.5 0 0 0-1.4-.9H5a1.8 1.8 0 1 1 0-3.6h.1a1.5 1.5 0 0 0 1.4-1 1.5 1.5 0 0 0-.3-1.7l-.1-.1a1.8 1.8 0 1 1 2.6-2.6l.1.1a1.5 1.5 0 0 0 1.7.3h0a1.5 1.5 0 0 0 .9-1.4V5a1.8 1.8 0 1 1 3.6 0v.1a1.5 1.5 0 0 0 .9 1.4 1.5 1.5 0 0 0 1.7-.3l.1-.1a1.8 1.8 0 1 1 2.6 2.6l-.1.1a1.5 1.5 0 0 0-.3 1.7v0a1.5 1.5 0 0 0 1.4.9H19a1.8 1.8 0 1 1 0 3.6h-.1a1.5 1.5 0 0 0-1.4.9Z",
  plus: "M12 5v14M5 12h14", send: "M12 19V5M5.5 11.5 12 5l6.5 6.5", stop: "M7 7h10v10H7z", clip: "M20 11.5 12.2 19.3a5 5 0 0 1-7-7l8-8a3.3 3.3 0 0 1 4.7 4.7l-8 8a1.7 1.7 0 0 1-2.4-2.4l7.4-7.4",
  x: "M6 6l12 12M18 6 6 18", search: "M11 18a7 7 0 1 0 0-14 7 7 0 0 0 0 14ZM20 20l-4-4", edit: "M4 20h4L19 9a2.8 2.8 0 0 0-4-4L4 16v4ZM13.5 6.5l4 4",
  trash: "M4 7h16M9 7V4h6v3M6 7l1 13h10l1-13M10 11v6M14 11v6", archive: "M4 5h16v4H4zM5 9v10h14V9M10 13h4", restore: "M4 12a8 8 0 1 1 3 6.2M4 19v-5h5",
  copy: "M9 9h10v11H9zM5 15V4h10", check: "M5 12.5l4.5 4.5L19 7.5", file: "M7 3h7l5 5v13H7zM14 3v5h5", image: "M4 5h16v14H4zM4 16l5-5 4 4 3-3 4 4M9 9.5h.01", chevron: "M9 6l6 6-6 6",
  down: "M12 4v12M6.5 11 12 16.5 17.5 11M5 20h14", brain: "M9 5a3 3 0 0 0-3 3 3 3 0 0 0-2 3 3 3 0 0 0 1.5 2.6A3 3 0 0 0 9 19V5ZM15 5a3 3 0 0 1 3 3 3 3 0 0 1 2 3 3 3 0 0 1-1.5 2.6A3 3 0 0 1 15 19V5Z",
  layers: "M12 3 3 8l9 5 9-5-9-5ZM3 13l9 5 9-5", chat: "M4 5h16v11H9l-5 4V5Z", palette: "M12 3a9 9 0 1 0 0 18c1.4 0 2-1 2-2s-.6-1.600.4-2.500c.9-.8 2.200 0 3.800-.4A5 5 0 0 0 21 11 9 9 0 0 0 12 3ZM7.500 11h.01M10 7.500h.01M14.500 7.500h.01",
  toggle: "M3 8h18v8H3zM15 12h.01", doc: "M6 3h9l4 4v14H6zM9 12h7M9 16h7M9 8h3", globe: "M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18ZM3 12h18M12 3c2.500 2.500 3.500 5.500 3.500 9s-1 6.500-3.500 9c-2.500-2.500-3.500-5.500-3.500-9S9.500 5.500 12 3Z",
  lock: "M6 11h12v9H6zM8.500 11V8a3.500 3.500 0 0 1 7 0v3", save: "M5 4h11l3 3v13H5zM8 4v5h7V4M8 20v-6h8v6", sliders: "M5 6h9M18 6h1M5 12h3M12 12h7M5 18h11M20 18h-1M14 4v4M8 10v4M16 16v4",
  zip: "M8 3h8l4 4v14H4V3h4ZM12 3v8M12 6h2M10 9h2M12 12h2", alert: "M12 4 2.500 20h19L12 4ZM12 10v4.500M12 17.500h.01", info: "M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18ZM12 11v5M12 8h.01", cpu: "M8 8h8v8H8zM4 9h2M4 15h2M18 9h2M18 15h2M9 4v2M15 4v2M9 18v2M15 18v2",
};
export type IconName = keyof typeof P;
export function Icon({ name, size = 18 }: { name: IconName; size?: number }) {
  return <svg className="ico" width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d={P[name]} /></svg>;
}

export function IconButton({ icon, label, active, size, ...r }: { icon: IconName; label: string; active?: boolean; size?: number } & ButtonHTMLAttributes<HTMLButtonElement>) {
  return <button type="button" {...r} className={`ib ${active ? "on" : ""} ${r.className ?? ""}`} aria-label={label} title={label}><Icon name={icon} size={size} /></button>;
}
export function Button({ variant = "default", icon, children, busy, ...r }: { variant?: "default" | "primary" | "danger" | "quiet"; icon?: IconName; busy?: boolean } & ButtonHTMLAttributes<HTMLButtonElement>) {
  return <button type="button" {...r} className={`btn ${variant} ${r.className ?? ""}`} disabled={r.disabled || busy}>{busy ? <Spinner /> : icon ? <Icon name={icon} size={16} /> : null}{children}</button>;
}
export function Spinner() { return <span className="spinner" role="status" aria-label="Working" />; }
export function Switch({ checked, onChange, label }: { checked: boolean; onChange: (v: boolean) => void; label: string }) {
  return <button type="button" role="switch" aria-checked={checked} aria-label={label} className={`switch ${checked ? "on" : ""}`} onClick={() => onChange(!checked)}><i /></button>;
}
export function Segmented<T extends string>({ value, options, onChange, label }: { value: T; options: Array<[T, string]>; onChange: (v: T) => void; label: string }) {
  return <div className="seg" role="radiogroup" aria-label={label}>{options.map(([v, l]) => <button key={v} type="button" role="radio" aria-checked={value === v} className={value === v ? "on" : ""} onClick={() => onChange(v)}>{l}</button>)}</div>;
}
export function Slider({ value, min, max, step, onChange, format, label }: { value: number; min: number; max: number; step: number; onChange: (v: number) => void; format: (v: number) => string; label: string }) {
  return <span className="slider"><input type="range" aria-label={label} min={min} max={max} step={step} value={value} onChange={(e) => onChange(Number(e.target.value))} /><output>{format(value)}</output></span>;
}
export function Setting({ title, hint, children }: { title: string; hint?: ReactNode; children: ReactNode }) {
  return <div className="setting"><div className="st"><b>{title}</b>{hint && <small>{hint}</small>}</div><div className="sc">{children}</div></div>;
}
export function Card({ title, hint, children, actions }: { title?: string; hint?: ReactNode; children: ReactNode; actions?: ReactNode }) {
  return <section className="card">{(title || actions) && <header><div>{title && <h3>{title}</h3>}{hint && <p className="hint">{hint}</p>}</div>{actions}</header>}{children}</section>;
}
export function Notice({ kind = "info", children, action }: { kind?: "info" | "error" | "warn" | "ok"; children: ReactNode; action?: ReactNode }) {
  return <div className={`notice ${kind}`} role={kind === "error" ? "alert" : "status"}><Icon name={kind === "error" || kind === "warn" ? "alert" : kind === "ok" ? "check" : "info"} size={16} /><div>{children}</div>{action}</div>;
}
export function Empty({ icon, title, children }: { icon: IconName; title: string; children?: ReactNode }) {
  return <div className="empty"><span className="emblem"><Icon name={icon} size={22} /></span><b>{title}</b>{children && <p>{children}</p>}</div>;
}
export function Skeleton({ lines = 3 }: { lines?: number }) { return <div className="skeleton" aria-hidden="true">{Array.from({ length: lines }, (_, i) => <i key={i} style={{ width: `${90 - i * 18}%` }} />)}</div>; }

/** Copy button with a 1.4 s confirmation. */
export function CopyButton({ text, label = "Copy", iconOnly }: { text: string; label?: string; iconOnly?: boolean }) {
  const [done, setDone] = useState(false); const t = useRef<number>(0);
  useEffect(() => () => window.clearTimeout(t.current), []);
  const copy = async () => { try { await navigator.clipboard.writeText(text); setDone(true); window.clearTimeout(t.current); t.current = window.setTimeout(() => setDone(false), 1400); } catch { /* clipboard unavailable */ } };
  return iconOnly ? <IconButton icon={done ? "check" : "copy"} label={done ? "Copied" : label} onClick={copy} size={16} /> : <button type="button" className="copy" onClick={copy}><Icon name={done ? "check" : "copy"} size={14} />{done ? "Copied" : label}</button>;
}
