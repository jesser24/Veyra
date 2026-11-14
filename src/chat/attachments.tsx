import { useCallback, useEffect, useRef, useState } from "react";
import { files, fmtBytes, type Analysis, type AttachKind, type AttachmentRow } from "../lib/ai";
import { Icon, IconButton, Spinner, type IconName } from "../components/ui";

export const MAX_FILES = 10; export const MAX_BYTES = 50 << 20;
export const ACCEPT = ".png,.jpg,.jpeg,.webp,.gif,.bmp,.svg,.pdf,.docx,.xlsx,.xlsm,.pptx,.csv,.tsv,.txt,.md,.markdown,.zip,.7z,.rar,.json,.xml,.yaml,.yml,.toml,.html,.css,.js,.jsx,.ts,.tsx,.py,.rs,.go,.java,.c,.h,.cpp,.cs,.rb,.php,.sh,.sql,.log";
export const KIND_LABEL: Record<AttachKind, string> = { image: "Image", pdf: "PDF", word: "Word", sheet: "Spreadsheet", slides: "Presentation", csv: "CSV", text: "Text", code: "Code", archive: "Archive", unsupported: "File" };
const KIND_ICON: Record<AttachKind, IconName> = { image: "image", pdf: "doc", word: "doc", sheet: "layers", slides: "layers", csv: "layers", text: "file", code: "file", archive: "zip", unsupported: "file" };

export interface Pending { key: string; name: string; size: number; state: "staging" | "ready" | "error"; analysis?: Analysis; error?: string; thumb?: string }

/** Staging area for the files of the message being written. Analysis runs in Rust; here we only show state and enforce the cheap limits early. */
export function useStaging() {
  const [items, setItems] = useState<Pending[]>([]); const itemsRef = useRef<Pending[]>([]); itemsRef.current = items;
  const add = useCallback(async (list: File[]) => {
    const room = MAX_FILES - itemsRef.current.length; const take = list.slice(0, Math.max(0, room));
    const extra: Pending[] = list.slice(take.length).map((f, i) => ({ key: `x${Date.now()}${i}`, name: f.name, size: f.size, state: "error", error: `At most ${MAX_FILES} files per message.` }));
    const mine: Pending[] = take.map((f, i) => ({ key: `${Date.now()}-${i}-${f.name}`, name: f.name || "pasted-image.png", size: f.size, state: f.size > MAX_BYTES ? "error" : "staging", error: f.size > MAX_BYTES ? `Larger than ${MAX_BYTES >> 20} MB.` : undefined }));
    setItems((p) => [...p, ...mine, ...extra]);
    for (let i = 0; i < take.length; i++) {
      const p = mine[i]; if (p.state === "error") continue; const f = take[i];
      const thumb = f.type.startsWith("image/") ? URL.createObjectURL(f) : undefined;
      try { const a = await files.stage(f); setItems((q) => q.map((x) => (x.key === p.key ? { ...x, state: "ready", analysis: a, thumb } : x))); }
      catch (e) { if (thumb) URL.revokeObjectURL(thumb); setItems((q) => q.map((x) => (x.key === p.key ? { ...x, state: "error", error: String(e) } : x))); }
    }
  }, []);
  const remove = useCallback((key: string) => { setItems((p) => { const x = p.find((i) => i.key === key); if (x?.analysis) void files.discard(x.analysis.token).catch(() => {}); if (x?.thumb) URL.revokeObjectURL(x.thumb); return p.filter((i) => i.key !== key); }); }, []);
  /** After a successful send the backend consumed the tokens; just forget the chips. */
  const clear = useCallback(() => { setItems((p) => { p.forEach((x) => x.thumb && URL.revokeObjectURL(x.thumb)); return []; }); }, []);
  const tokens = items.filter((i) => i.state === "ready" && i.analysis).map((i) => i.analysis!.token);
  return { items, add, remove, clear, tokens, staging: items.some((i) => i.state === "staging") };
}

/** Files picked, dropped or pasted -> File[] (images pasted from the clipboard have no name). */
export function filesFrom(dt: DataTransfer | null): File[] { if (!dt) return []; return Array.from(dt.files ?? []).map((f) => (f.name ? f : new File([f], `image-${Date.now()}.${(f.type.split("/")[1] || "png").replace("+xml", "")}`, { type: f.type }))); }

export function Chip({ p, onRemove, onOpen }: { p: Pending; onRemove: () => void; onOpen?: () => void }) {
  const a = p.analysis; const kind = a?.kind ?? "unsupported";
  return (
    <div className={`chip ${p.state}`} title={p.error ?? a?.notes.join(" ") ?? p.name}>
      <button type="button" className="chip-main" onClick={onOpen} disabled={!a} aria-label={`Preview ${p.name}`}>
        <span className="chip-thumb">{p.thumb ? <img src={p.thumb} alt="" /> : p.state === "staging" ? <Spinner /> : <Icon name={p.state === "error" ? "alert" : KIND_ICON[kind]} size={18} />}</span>
        <span className="chip-text"><b>{p.name}</b><small>{p.state === "error" ? p.error : p.state === "staging" ? "Reading…" : `${KIND_LABEL[kind]} · ${a?.summary || fmtBytes(p.size)}`}</small></span>
      </button>
      <IconButton icon="x" label={`Remove ${p.name}`} size={14} onClick={onRemove} />
    </div>
  );
}

/** Blob URL for a stored image, fetched once. Revoked on unmount. */
export function useImageUrl(id: string | null, mime: string, source: "stored" | "staged" = "stored"): string | null {
  const [url, setUrl] = useState<string | null>(null);
  useEffect(() => { if (!id) return; let dead = false; let u: string | null = null;
    (source === "stored" ? files.stored(id) : files.staged(id)).then((b) => { if (dead) return; u = URL.createObjectURL(new Blob([b], { type: mime })); setUrl(u); }).catch(() => {});
    return () => { dead = true; if (u) URL.revokeObjectURL(u); }; }, [id, mime, source]);
  return url;
}

export function StoredAttachment({ a, onOpen }: { a: AttachmentRow; onOpen: (a: AttachmentRow) => void }) {
  const img = a.kind === "image" || a.mime === "image/svg+xml"; const url = useImageUrl(img ? a.id : null, a.mime);
  return (
    <button type="button" className={`att ${img ? "img" : ""}`} onClick={() => onOpen(a)} title={a.name}>
      {img ? (url ? <img src={url} alt={a.name} loading="lazy" /> : <span className="ph"><Spinner /></span>) : <><span className="att-ic"><Icon name={KIND_ICON[a.kind] ?? "file"} size={18} /></span><span className="att-tx"><b>{a.name}</b><small>{KIND_LABEL[a.kind]} · {a.summary || fmtBytes(a.size)}</small></span></>}
    </button>
  );
}
