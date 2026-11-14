import { useCallback, useEffect, useRef, useState } from "react";
import { ai, chat, type AttachmentRow, type Conversation, type Message } from "../lib/ai";
import { api } from "../lib/api";
import { useListen } from "../lib/hooks";

/** One conversation session. The data lives in the backend; this hook only mirrors it, so the application window and the chat window agree. */
export function useChat() {
  const [cur, setCur] = useState<Conversation | null>(null); const [msgs, setMsgs] = useState<Message[]>([]); const [atts, setAtts] = useState<AttachmentRow[]>([]);
  const [busy, setBusy] = useState<string | null>(null); const [err, setErr] = useState<string | null>(null);
  const curId = useRef<string | null>(null); const buf = useRef<{ id: string; text: string } | null>(null); const raf = useRef(0);

  const load = useCallback(async (id: string) => {
    const [m, a] = await Promise.all([chat.messages(id), chat.attachments(id).catch(() => [])]);
    if (curId.current !== id) return; setMsgs(m); setAtts(a);
    const pending = [...m].reverse().find((x) => x.role === "assistant" && x.status === "pending");
    setBusy(pending && (await ai.state().then((s) => s.generating).catch(() => false)) ? pending.id : null);
  }, []);
  const open = useCallback(async (c: Conversation | string) => {
    const id = typeof c === "string" ? c : c.id; curId.current = id; setErr(null); setBusy(null);
    if (typeof c !== "string") setCur(c);
    try { await load(id); if (typeof c === "string") setCur(await chat.list("active").then((l) => l.find((x) => x.id === id) ?? null).then(async (f) => f ?? (await chat.list("archived")).find((x) => x.id === id) ?? null)); } catch (e) { setErr(String(e)); }
  }, [load]);
  const fresh = useCallback(() => { curId.current = null; setCur(null); setMsgs([]); setAtts([]); setErr(null); setBusy(null); }, []);

  // Streaming: deltas are collected and painted at most once per frame, so a fast model never makes the interface stutter.
  const flush = useCallback(() => { raf.current = 0; const b = buf.current; if (!b) return; buf.current = null; setMsgs((m) => m.map((x) => (x.id === b.id ? { ...x, content: x.content + b.text } : x))); }, []);
  useListen(() => chat.onToken((e) => { if (e.conversation !== curId.current) return; const b = buf.current; if (b && b.id === e.message) b.text += e.delta; else { flush(); buf.current = { id: e.message, text: e.delta }; } if (!raf.current) raf.current = requestAnimationFrame(flush); }), [flush]);
  useListen(() => chat.onDone((e) => { if (e.conversation !== curId.current) return; flush(); setBusy(null); void load(e.conversation); }), [load, flush]);
  useListen(() => chat.onError((e) => { if (e.conversation !== curId.current) return; flush(); setBusy(null); setErr(e.error); void load(e.conversation); }), [load, flush]);
  // another window changed this conversation (renamed, deleted): follow
  useListen(() => chat.onListChanged(() => { const id = curId.current; if (!id) return; void chat.list("active").then(async (l) => { const f = l.find((x) => x.id === id); if (f) setCur((p) => (p ? { ...p, title: f.title } : p)); else if (!(await chat.list("archived")).some((x) => x.id === id)) fresh(); }).catch(() => {}); }), [fresh]);
  useListen(() => api.onOpenConversation((id) => { if (id) void open(id); else fresh(); }), [open, fresh]);
  useEffect(() => { void api.takePendingOpen().then((id) => { if (id) void open(id); }).catch(() => {}); }, [open]);

  const send = useCallback(async (text: string, tokens: string[]) => {
    setErr(null);
    const r = await chat.send(curId.current, text, tokens); // throws with a readable message (e.g. no model loaded); the caller keeps the draft
    curId.current = r.conversation.id; setCur(r.conversation); setMsgs((m) => [...(m.length && m[0].conversation_id === r.conversation.id ? m : []), r.user_message, r.assistant_message]); setBusy(r.assistant_message.id);
    chat.attachments(r.conversation.id).then((a) => { if (curId.current === r.conversation.id) setAtts(a); }).catch(() => {});
  }, []);
  const stop = useCallback(() => { void ai.cancel(); }, []);
  return { cur, msgs, atts, busy, err, setErr, open, fresh, send, stop, setCur };
}
