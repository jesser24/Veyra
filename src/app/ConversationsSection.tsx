import { useState } from "react";
import { chat, type Conversation } from "../lib/ai";
import { api } from "../lib/api";
import { useConversations, type View } from "../lib/hooks";
import { Button, Card, Empty, IconButton, Notice, Segmented, Skeleton } from "../components/ui";

export function ConversationsSection() {
  const [view, setView] = useState<View>("active"); const [q, setQ] = useState(""); const [list, refresh] = useConversations(view, q);
  const [renaming, setRenaming] = useState<string | null>(null); const [title, setTitle] = useState(""); const [err, setErr] = useState<string | null>(null); const [confirm, setConfirm] = useState<string | null>(null);
  const run = (f: () => Promise<unknown>) => f().then(() => { setErr(null); refresh(); }).catch((e) => setErr(String(e)));
  const commit = async (c: Conversation) => { const t = title.trim(); setRenaming(null); if (t && t !== c.title) await run(() => chat.rename(c.id, t)); };
  const when = (t: number) => new Date(t * 1000).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });
  return (
    <div className="stack">
      <div className="toolbar"><input className="field grow" type="search" placeholder="Search all conversations" value={q} onChange={(e) => setQ(e.target.value)} aria-label="Search conversations" />
        <Segmented label="Show" value={view} onChange={(v) => { setView(v); setConfirm(null); }} options={[["active", "Recent"], ["archived", "Archived"], ["trash", "Trash"]]} /></div>
      {err && <Notice kind="error">{err}</Notice>}
      <Card>
        {list === null && <Skeleton lines={4} />}
        {list && list.length === 0 && <Empty icon="chat" title={q ? "No match" : view === "trash" ? "Trash is empty" : view === "archived" ? "Nothing archived" : "No conversations yet"}>{view === "active" && !q ? "Start one from the chat window." : undefined}</Empty>}
        <ul className="rows">{list?.map((c) => (
          <li key={c.id}>
            <div className="row-main">{renaming === c.id
              ? <input className="field" autoFocus value={title} onChange={(e) => setTitle(e.target.value)} onBlur={() => void commit(c)} onKeyDown={(e) => { if (e.key === "Enter") void commit(c); if (e.key === "Escape") setRenaming(null); }} aria-label="Title" />
              : <><b>{c.title}</b><small>{when(c.updated)}</small></>}</div>
            <div className="row-acts">
              {view === "trash" ? (confirm === c.id ? <><Button variant="danger" onClick={() => run(() => chat.remove(c.id))}>Delete forever</Button><Button onClick={() => setConfirm(null)}>Cancel</Button></> : <><Button icon="restore" onClick={() => run(() => chat.restore(c.id))}>Restore</Button><IconButton icon="trash" label="Delete permanently" onClick={() => setConfirm(c.id)} /></>)
                : <><Button variant="primary" onClick={() => void api.openChat(c.id)}>Open in chat</Button><IconButton icon="edit" label="Rename" onClick={() => { setRenaming(c.id); setTitle(c.title); }} />
                  <IconButton icon={view === "archived" ? "restore" : "archive"} label={view === "archived" ? "Unarchive" : "Archive"} onClick={() => run(() => chat.archive(c.id, view !== "archived"))} /><IconButton icon="trash" label="Move to trash" onClick={() => run(() => chat.trash(c.id))} /></>}
            </div>
          </li>))}</ul>
      </Card>
    </div>
  );
}
