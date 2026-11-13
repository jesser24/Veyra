import { useCallback, useEffect, useRef, useState } from "react";
import { ai, chat, fmtMs, fmtRate, type Conversation, type Message, type Perf } from "../lib/ai";
import { CopyButton, Md } from "./Md";

type View = "active" | "archived" | "trash";
interface Props { modelLoaded: boolean; onOpenAi: () => void }

export function Chat({ modelLoaded, onOpenAi }: Props) {
  const [convs, setConvs] = useState<Conversation[]>([]); const [view, setView] = useState<View>("active"); const [q, setQ] = useState(""); const [list, setList] = useState(false);
  const [cur, setCur] = useState<Conversation | null>(null); const [msgs, setMsgs] = useState<Message[]>([]); const [text, setText] = useState("");
  const [busy, setBusy] = useState<string | null>(null); /* id of the assistant message being generated */ const [err, setErr] = useState<string | null>(null);
  const [perf, setPerf] = useState<Perf | null>(null); const [thinking, setThinking] = useState(false); const [renaming, setRenaming] = useState<string | null>(null); const [title, setTitle] = useState("");
  const curRef = useRef<string | null>(null); const endRef = useRef<HTMLDivElement>(null);
  curRef.current = cur?.id ?? null;

  const refresh = useCallback(() => chat.list(view, q).then(setConvs).catch((e) => setErr(String(e))), [view, q]);
  useEffect(() => { void refresh(); }, [refresh]);
  useEffect(() => { endRef.current?.scrollIntoView({ block: "end" }); }, [msgs]);
  useEffect(() => {
    const offs: Array<Promise<() => void>> = [
      chat.onToken((e) => { if (e.conversation === curRef.current) setMsgs((m) => m.map((x) => x.id === e.message ? { ...x, content: x.content + e.delta } : x)); }),
      chat.onDone((e) => { setBusy(null); setPerf(e.cancelled ? null : e.perf); if (e.conversation === curRef.current) void chat.messages(e.conversation).then(setMsgs); void refresh(); }),
      chat.onError((e) => { setBusy(null); setErr(e.error); if (e.conversation === curRef.current) void chat.messages(e.conversation).then(setMsgs); }),
    ];
    return () => { offs.forEach((p) => void p.then((f) => f())); };
  }, [refresh]);

  const open = async (c: Conversation) => { setCur(c); setList(false); setErr(null); setPerf(null); setMsgs(await chat.messages(c.id)); };
  const fresh = () => { setCur(null); setMsgs([]); setList(false); setErr(null); setPerf(null); };
  const send = async () => {
    const t = text.trim(); if (!t || busy) return; setErr(null); setText("");
    try {
      const r = await chat.send(cur?.id ?? null, t, thinking); setCur(r.conversation); curRef.current = r.conversation.id;
      setMsgs((m) => [...(cur ? m : []), r.user_message, r.assistant_message]); setBusy(r.assistant_message.id); void refresh();
    } catch (e) { setErr(String(e)); setText(t); }
  };
  const doRename = async (id: string) => { const t = title.trim(); setRenaming(null); if (t) { await chat.rename(id, t).catch((e) => setErr(String(e))); if (cur?.id === id) setCur({ ...cur, title: t }); void refresh(); } };

  return (
    <div className="chat">
      <div className="chatbar">
        <button onClick={() => setList(!list)} aria-expanded={list} aria-label="Conversations">☰ {cur ? cur.title : "New conversation"}</button>
        <span className="sp" /><button onClick={fresh} aria-label="New conversation">＋ New</button>
      </div>
      {list ? (
        <div className="convlist">
          <input type="search" placeholder="Search conversations" value={q} onChange={(e) => setQ(e.target.value)} aria-label="Search conversations" />
          <div className="tabs2" role="tablist">{(["active", "archived", "trash"] as View[]).map((v) => <button key={v} role="tab" aria-selected={view === v} className={view === v ? "on" : ""} onClick={() => setView(v)}>{v === "active" ? "Recent" : v === "archived" ? "Archived" : "Trash"}</button>)}</div>
          {convs.length === 0 && <p className="muted">{q ? "No match." : "Nothing here."}</p>}
          {convs.map((c) => (
            <div key={c.id} className={`conv ${cur?.id === c.id ? "sel" : ""}`}>
              {renaming === c.id ? <input autoFocus value={title} onChange={(e) => setTitle(e.target.value)} onBlur={() => void doRename(c.id)} onKeyDown={(e) => { if (e.key === "Enter") void doRename(c.id); if (e.key === "Escape") setRenaming(null); }} aria-label="New title" />
                : <button className="ctitle" onClick={() => void open(c)}>{c.title}</button>}
              <span className="acts">
                {view === "trash" ? <><button onClick={() => chat.restore(c.id).then(refresh)}>Restore</button><button onClick={() => chat.remove(c.id).then(refresh)}>Delete forever</button></>
                  : <><button aria-label="Rename" onClick={() => { setRenaming(c.id); setTitle(c.title); }}>✎</button><button aria-label={view === "archived" ? "Unarchive" : "Archive"} onClick={() => chat.archive(c.id, view !== "archived").then(refresh)}>{view === "archived" ? "↩" : "⌂"}</button><button aria-label="Move to trash" onClick={() => { void chat.trash(c.id).then(() => { if (cur?.id === c.id) fresh(); void refresh(); }); }}>🗑</button></>}
              </span>
            </div>))}
        </div>
      ) : (
        <>
          <div className="msgs" aria-live="polite">
            {msgs.length === 0 && <div className="empty"><b>Ask anything.</b><p>{modelLoaded ? "Everything stays on this computer." : <>No model is loaded yet. <button className="link" onClick={onOpenAi}>Set up the local model</button></>}</p></div>}
            {msgs.map((m) => (
              <div key={m.id} className={`msg ${m.role} ${m.status}`}>
                {m.role === "user" ? <p className="ut">{m.content}</p> : (
                  <>{m.content ? <Md text={m.content} /> : busy === m.id ? <span className="dots" aria-label="Generating">● ● ●</span> : null}
                    {m.status === "error" && <p className="err">This reply was interrupted{m.content ? " (partial text kept)" : ""}.</p>}
                    {m.content && busy !== m.id && <div className="macts"><CopyButton text={m.content} label="Copy reply" /></div>}</>)}
              </div>))}
            <div ref={endRef} />
          </div>
          {err && <p className="err" role="alert">{err}</p>}
          {perf && !busy && <p className="perf">{perf.generated_tokens ?? "?"} tokens · {fmtRate(perf.generated_tokens_per_s)} · first token {fmtMs(perf.first_token_ms)} · measured by the engine</p>}
          <form className="compose" onSubmit={(e) => { e.preventDefault(); void send(); }}>
            <textarea value={text} rows={2} placeholder={modelLoaded ? "Message Veyra…" : "Load a model first"} aria-label="Message" disabled={!modelLoaded}
              onChange={(e) => setText(e.target.value)} onKeyDown={(e) => { if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); void send(); } }} />
            <div className="cbtns">
              <label className="check small" title="Lets the model reason before answering: slower, sometimes better"><input type="checkbox" checked={thinking} onChange={(e) => setThinking(e.target.checked)} /> Thinking</label>
              {busy ? <button type="button" className="stop" onClick={() => void ai.cancel()}>■ Stop</button> : <button className="primary" disabled={!text.trim() || !modelLoaded}>Send</button>}
            </div>
          </form>
        </>
      )}
    </div>
  );
}
