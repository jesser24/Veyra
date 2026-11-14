import { fmtBytes, type AttachmentRow } from "../lib/ai";
import { IconButton } from "../components/ui";
import { KIND_LABEL, useImageUrl } from "./attachments";

export interface PreviewItem { id: string; name: string; mime: string; size: number; kind: string; summary: string; notes: string[]; source: "stored" | "staged"; width?: number | null; height?: number | null }
export const fromRow = (a: AttachmentRow): PreviewItem => ({ id: a.id, name: a.name, mime: a.mime, size: a.size, kind: a.kind, summary: a.summary, notes: (() => { try { return JSON.parse(a.notes) as string[]; } catch { return []; } })(), source: "stored", width: a.width, height: a.height });

/** Lightbox for one attachment. Images are shown as pictures (an SVG is an <img>, so its scripts never run); documents show what Veyra extracted and what it could not. */
export function Preview({ item, onClose }: { item: PreviewItem; onClose: () => void }) {
  const img = item.kind === "image" || item.mime === "image/svg+xml"; const url = useImageUrl(img ? item.id : null, item.mime, item.source);
  return (
    <div className="modal-scrim" onClick={onClose} role="presentation">
      <div className="modal" role="dialog" aria-modal="true" aria-label={item.name} onClick={(e) => e.stopPropagation()}>
        <header><div><b>{item.name}</b><small>{KIND_LABEL[item.kind as keyof typeof KIND_LABEL] ?? "File"} · {fmtBytes(item.size)}{item.width ? ` · ${item.width}×${item.height}` : ""}</small></div><IconButton icon="x" label="Close preview" onClick={onClose} /></header>
        {img && <div className="modal-img">{url ? <img src={url} alt={item.name} /> : null}</div>}
        {item.summary && <p className="modal-sum">{item.summary}</p>}
        {item.notes.map((n, i) => <p key={i} className="modal-note">{n}</p>)}
      </div>
    </div>
  );
}
