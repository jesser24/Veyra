import { memo, useCallback, useEffect, useRef, useState, type ClipboardEvent, type KeyboardEvent } from "react";
import { listen } from "@tauri-apps/api/event";
import { api } from "../lib/api";
import { ai, type AttachmentRow, type DlEvent, type Message } from "../lib/ai";
import { useEngine, useListen } from "../lib/hooks";
import { useSettings } from "../lib/useSettings";
import { Button, CopyButton, Icon, IconButton, Notice } from "../components/ui";
import { Md } from "../components/Md";
import { useChat } from "./useChat";
import { ACCEPT, Chip, StoredAttachment, filesFrom, useStaging } from "./attachments";
import { Preview, fromRow, type PreviewItem } from "./Preview";
import { History } from "./History";
import { Setup } from "../app/Setup";

const Bubble = memo(function Bubble({ m, atts, streaming, onOpen }: { m: Message; atts: AttachmentRow[]; streaming: boolean; onOpen: (a: AttachmentRow) => void }) {
  if (m.role === "user") return (
    <div className="turn user">
      <div className="bubble">
        {atts.length > 0 && <div className="atts">{atts.map((a) => <StoredAttachment key={a.id} a={a} onOpen={onOpen} />)}</div>}
        {m.content && <p className="ut">{m.content}</p>}
      </div>
    </div>
  );
  return (
    <div className="turn assistant">
      <div className="amark" aria-hidden="true"><img src="/veyra-mark.png" alt="" draggable={false} /></div>
      <div className="abody">
        {m.content ? <><Md text={m.content} />{streaming && <span className="caret" aria-hidden="true" />}</> : streaming ? <span className="typing" role="status" aria-label="Veyra is answering"><i /><i /><i /></span> : null}
        {m.status === "error" && !streaming && <p className="interrupted"><Icon name="alert" size={14} />Interrupted{m.content ? " — the partial answer is kept" : ""}</p>}
        {m.content && !streaming && <div className="macts"><CopyButton text={m.content} label="Copy" /></div>}
      </div>
    </div>
  );
});

export function ChatWindow() {
  const s = useSettings(); const eng = useEngine(); const c = useChat(); const stage = useStaging();
  const [text, setText] = useState(""); const [hist, setHist] = useState(false); const [preview, setPreview] = useState<PreviewItem | null>(null);
  const [open, setOpen] = useState(true); const [collapsed, setCollapsed] = useState(false); const [drag, setDrag] = useState(false); const [dl, setDl] = useState<DlEvent | null>(null);
  const [atBottom, setAtBottom] = useState(true);
  const scroller = useRef<HTMLDivElement>(null); const ta = useRef<HTMLTextAreaElement>(null); const picker = useRef<HTMLInputElement>(null); const depth = useRef(0);

  useListen(() => listen<boolean>("panel-visibility", (e) => setOpen(e.payload)), []);
  useListen(() => api.onCollapsed(setCollapsed), []);
  useListen(() => ai.onDownload(setDl), []);
  useEffect(() => { if (open) ta.current?.focus(); }, [open, collapsed]);

  // keyboard: Esc peels back one layer at a time (preview, history, then hides the window); Ctrl/Cmd+N starts a new conversation
  useEffect(() => {
    const k = (e: globalThis.KeyboardEvent) => {
      if (e.key === "Escape") { if (preview) { setPreview(null); ta.current?.focus(); } else if (hist) setHist(false); else void api.hidePanel(); }
      else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "n") { e.preventDefault(); c.fresh(); setText(""); stage.clear(); ta.current?.focus(); }
    };
    window.addEventListener("keydown", k); return () => window.removeEventListener("keydown", k);
  }, [preview, hist, c, stage]);

  // follow the stream unless the reader scrolled up to read something
  useEffect(() => { const el = scroller.current; if (el && atBottom) el.scrollTop = el.scrollHeight; }, [c.msgs, c.busy, atBottom]);
  const onScroll = () => { const el = scroller.current; if (el) setAtBottom(el.scrollHeight - el.scrollTop - el.clientHeight < 80); };
  const toBottom = () => { const el = scroller.current; if (el) el.scrollTo({ top: el.scrollHeight, behavior: "smooth" }); setAtBottom(true); };
  useEffect(() => { const el = ta.current; if (!el) return; el.style.height = "0px"; el.style.height = `${Math.min(el.scrollHeight, 168)}px`; }, [text]);

  const ready = !!eng?.status.loaded; const loading = !!eng?.loading; const generating = !!c.busy;
  const canSend = ready && !generating && !stage.staging && (text.trim().length > 0 || stage.tokens.length > 0);
  const send = useCallback(async () => {
    if (!canSend) return; const t = text; const toks = stage.tokens; setText(""); setAtBottom(true);
    try { await c.send(t.trim(), toks); stage.clear(); } catch (e) { setText(t); c.setErr(String(e)); }
  }, [canSend, text, stage, c]);
  const onKey = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key !== "Enter" || e.nativeEvent.isComposing) return;
    const enterSends = s?.ui.enter_sends ?? true; const mod = e.ctrlKey || e.metaKey;
    if (enterSends ? !e.shiftKey && !mod : mod) { e.preventDefault(); void send(); }
  };
  const onPaste = (e: ClipboardEvent) => { const f = filesFrom(e.clipboardData); if (f.length) { e.preventDefault(); void stage.add(f); } };
  const onDrop = (e: React.DragEvent) => { e.preventDefault(); depth.current = 0; setDrag(false); const f = filesFrom(e.dataTransfer); if (f.length) void stage.add(f); };
  const onDragEnter = (e: React.DragEvent) => { if (e.dataTransfer.types.includes("Files")) { depth.current++; setDrag(true); } };
  const onDragLeave = () => { depth.current = Math.max(0, depth.current - 1); if (depth.current === 0) setDrag(false); };
  const openAtt = (a: AttachmentRow) => setPreview(fromRow(a));
  const pct = dl && dl.phase === "downloading" && dl.total ? Math.round((dl.done / dl.total) * 100) : null;
  const dlActive = !!eng?.downloading;

  if (!s) return null;
  if (!s.setup_done) return <div className="chatwin in"><header className="chead" data-tauri-drag-region><span className="grow" data-tauri-drag-region /></header><Setup compact /></div>;
  const byMsg = (id: string) => c.atts.filter((a) => a.message_id === id);
  return (
    <div className={`chatwin ${open ? "in" : "out"} ${collapsed ? "collapsed" : ""}`} role="application" aria-label="Veyra" onDragOver={(e) => e.preventDefault()} onDragEnter={onDragEnter} onDragLeave={onDragLeave} onDrop={onDrop}>
      <header className="chead" data-tauri-drag-region>
        <IconButton icon="history" label="Conversation history" active={hist} onClick={() => setHist((v) => !v)} aria-expanded={hist} />
        <div className="brand" data-tauri-drag-region><img src="/veyra-mark.png" alt="" draggable={false} data-tauri-drag-region /><b data-tauri-drag-region>Veyra</b></div>
        <span className="grow" data-tauri-drag-region />
        <IconButton icon="gear" label="Open Veyra settings" onClick={() => void api.openApp()} />
      </header>
      {!collapsed && <>
        <div className="stage">
          <div className="scroller" ref={scroller} onScroll={onScroll} aria-live="polite" aria-busy={generating}>
            <div className="thread">
              {c.msgs.length === 0 && (
                <div className="welcome">
                  <img src="/veyra-mark.png" alt="" draggable={false} />
                  <h1>{ready ? "How can I help?" : loading ? "Waking up…" : "Almost ready"}</h1>
                  <p>{ready ? "Private by design: everything runs on this computer." : loading ? "Loading the model into memory." : "Veyra needs a local model before it can answer."}</p>
                </div>)}
              {c.msgs.map((m) => <Bubble key={m.id} m={m} atts={m.role === "user" ? byMsg(m.id) : []} streaming={c.busy === m.id} onOpen={openAtt} />)}
            </div>
          </div>
          {!atBottom && c.msgs.length > 0 && <button type="button" className="tolatest" onClick={toBottom} aria-label="Scroll to the latest message"><Icon name="down" size={16} /></button>}
          <History open={hist} currentId={c.cur?.id ?? null} onClose={() => setHist(false)} onSelect={(x) => { void c.open(x); stage.clear(); }} onNew={() => { c.fresh(); setText(""); stage.clear(); ta.current?.focus(); }} onDeleted={(id) => { if (c.cur?.id === id) c.fresh(); }} />
          {drag && <div className="dropveil"><div><Icon name="clip" size={22} /><b>Drop to attach</b><small>Images, documents, archives, code</small></div></div>}
        </div>
        <footer className="dock">
          {eng?.stub && <Notice kind="error">This build of Veyra does not include its inference library.</Notice>}
          {eng && !eng.stub && !ready && loading && <Notice kind="info"><b>Loading the model…</b> You can already write your message.</Notice>}
          {eng && !eng.stub && !ready && !loading && dlActive && <Notice kind="info"><b>Downloading the model{pct != null ? ` — ${pct} %` : "…"}</b> It will load by itself when verified.</Notice>}
          {eng && !eng.stub && !ready && !loading && !dlActive && (eng.error
            ? <Notice kind="error" action={<Button onClick={() => void api.openApp("models")}>Models</Button>}>{eng.error.split("\n").map((l, i) => <p key={i}>{l}</p>)}</Notice>
            : <Notice kind="info" action={<Button variant="primary" onClick={() => void api.openApp("models")}>Set up</Button>}>{eng.selected ? "The selected model is not loaded." : "No model yet. Choose one to download."}</Notice>)}
          {c.err && <Notice kind="error" action={<IconButton icon="x" label="Dismiss" size={14} onClick={() => c.setErr(null)} />}>{c.err}</Notice>}
          {stage.items.length > 0 && <div className="tray" role="list" aria-label="Files to send">{stage.items.map((p) => <Chip key={p.key} p={p} onRemove={() => stage.remove(p.key)} onOpen={p.analysis && (() => setPreview({ id: p.analysis!.token, name: p.name, mime: p.analysis!.mime, size: p.size, kind: p.analysis!.kind, summary: p.analysis!.summary, notes: p.analysis!.notes, source: "staged", width: p.analysis!.image?.[0], height: p.analysis!.image?.[1] }))} />)}</div>}
          <form className="composer" onSubmit={(e) => { e.preventDefault(); void send(); }}>
            <input ref={picker} type="file" multiple accept={ACCEPT} hidden onChange={(e) => { const f = Array.from(e.target.files ?? []); e.target.value = ""; if (f.length) void stage.add(f); }} />
            <IconButton icon="clip" label="Attach files" onClick={() => picker.current?.click()} />
            <textarea ref={ta} rows={1} value={text} placeholder="Message Veyra" aria-label="Message" onChange={(e) => setText(e.target.value)} onKeyDown={onKey} onPaste={onPaste} />
            {generating
              ? <button type="button" className="sendbtn stop" onClick={c.stop} aria-label="Stop generating" title="Stop"><Icon name="stop" size={16} /></button>
              : <button type="submit" className="sendbtn" disabled={!canSend} aria-label="Send" title={ready ? "Send" : loading ? "Waiting for the model" : "No model loaded"}><Icon name="send" size={18} /></button>}
          </form>
        </footer>
      </>}
      {preview && <Preview item={preview} onClose={() => { setPreview(null); ta.current?.focus(); }} />}
    </div>
  );
}
