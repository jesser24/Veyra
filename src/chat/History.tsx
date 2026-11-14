import { useEffect, useRef, useState } from "react";
import { chat, type Conversation } from "../lib/ai";
import { groupByDay, useConversations, type View } from "../lib/hooks";
import { Empty, Icon, IconButton, Segmented, Skeleton } from "../components/ui";

/** Sliding history. It overlays the conversation (never pushes the composer away) and closes with Esc, the scrim, or by choosing a conversation. */
export function History({ open, currentId, onClose, onSelect, onNew, onDeleted }: { open: boolean; currentId: string | null; onClose: () => void; onSelect: (c: Conversation) => void; onNew: () => void; onDeleted: (id: string) => void }) {
  const [view, setView] = useState<View>("active"); const [q, setQ] = useState(""); const [list, refresh] = useConversations(view, q);
  const [renaming, setRenaming] = useState<string | null>(null); const [title, setTitle] = useState(""); const [confirm, setConfirm] = useState<string | null>(null); const [err, setErr] = useState<string | null>(null);
  const search = useRef<HTMLInputElement>(null); const root = useRef<HTMLElement>(null);
  useEffect(() => { if (open) { const t = window.setTimeout(() => search.current?.focus(), 180); return () => window.clearTimeout(t); } else { setRenaming(null); setConfirm(null); } }, [open]);
  const run = (f: () => Promise<unknown>) => f().then(() => { setErr(null); refresh(); }).catch((e) => setErr(String(e)));
  const commit = async (id: string) => { const t = title.trim(); setRenaming(null); if (t) await run(() => chat.rename(id, t)); };
  const groups = list ? (q ? [["Results", list] as [string, Conversation[]]] : groupByDay(list)) : [];
  return (
    <>
      <div className={`scrim ${open ? "on" : ""}`} onClick={onClose} aria-hidden="true" />
      <aside ref={root} className={`history ${open ? "on" : ""}`} aria-label="Conversation history" aria-hidden={!open} inert={!open}>
        <header><b>History</b><IconButton icon="x" label="Close history" onClick={onClose} /></header>
        <div className="h-tools">
          <button type="button" className="newchat" onClick={() => { onNew(); onClose(); }}><Icon name="plus" size={16} />New conversation</button>
          <label className="search"><Icon name="search" size={15} /><input ref={search} type="search" placeholder="Search" value={q} onChange={(e) => setQ(e.target.value)} aria-label="Search conversations" /></label>
        </div>
        <div className="h-list">
          {list === null && <Skeleton lines={4} />}
          {list && list.length === 0 && <Empty icon={q ? "search" : "chat"} title={q ? "No match" : view === "trash" ? "Trash is empty" : view === "archived" ? "Nothing archived" : "No conversations yet"}>{q ? "Try other words." : view === "active" ? "Your conversations will appear here." : undefined}</Empty>}
          {groups.map(([g, items]) => (
            <section key={g}><h4>{g}</h4>
              {items.map((c) => (
                <div key={c.id} className={`h-item ${c.id === currentId ? "sel" : ""}`}>
                  {renaming === c.id
                    ? <input className="rename" autoFocus value={title} onChange={(e) => setTitle(e.target.value)} onBlur={() => void commit(c.id)} onKeyDown={(e) => { if (e.key === "Enter") void commit(c.id); if (e.key === "Escape") { e.stopPropagation(); e.preventDefault(); setRenaming(null); } }} aria-label="Conversation title" />
                    : <button type="button" className="h-title" onClick={() => { onSelect(c); onClose(); }} title={c.title}>{c.title}</button>}
                  <span className="h-acts">
                    {view === "trash" ? (confirm === c.id
                      ? <><button className="mini danger" onClick={() => run(() => chat.remove(c.id))}>Delete forever</button><button className="mini" onClick={() => setConfirm(null)}>Cancel</button></>
                      : <><IconButton icon="restore" label="Restore" size={15} onClick={() => run(() => chat.restore(c.id))} /><IconButton icon="trash" label="Delete permanently" size={15} onClick={() => setConfirm(c.id)} /></>)
                      : <><IconButton icon="edit" label="Rename" size={15} onClick={() => { setRenaming(c.id); setTitle(c.title); }} />
                        <IconButton icon={view === "archived" ? "restore" : "archive"} label={view === "archived" ? "Unarchive" : "Archive"} size={15} onClick={() => run(() => chat.archive(c.id, view !== "archived"))} />
                        <IconButton icon="trash" label="Move to trash" size={15} onClick={() => run(async () => { await chat.trash(c.id); onDeleted(c.id); })} /></>}
                  </span>
                </div>))}
            </section>))}
          {err && <p className="inline-err" role="alert">{err}</p>}
        </div>
        <footer><Segmented label="Show" value={view} onChange={(v) => { setView(v); setConfirm(null); }} options={[["active", "Recent"], ["archived", "Archived"], ["trash", "Trash"]]} /></footer>
      </aside>
    </>
  );
}
