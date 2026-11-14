import { useCallback, useEffect, useRef, useState } from "react";
import { ai, chat, security, type Conversation, type EngineState, type SecurityStatus } from "./ai";

/** Subscribes to a Tauri event for the lifetime of the component (handles the async unlisten). */
export function useListen(sub: (() => Promise<() => void>) | null, deps: unknown[]) {
  useEffect(() => { if (!sub) return; let off: (() => void) | undefined; let dead = false; sub().then((f) => { if (dead) f(); else off = f; }); return () => { dead = true; off?.(); };
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps);
}

/** Engine state is owned by the backend and pushed to every window: the chat and the application always show the same thing. */
export function useEngine(): EngineState | null {
  const [s, setS] = useState<EngineState | null>(null);
  useEffect(() => { void ai.state().then(setS).catch(() => {}); }, []);
  useListen(() => ai.onState(setS), []);
  return s;
}
export function useSecurity(): [SecurityStatus | null, () => void] {
  const [s, setS] = useState<SecurityStatus | null>(null);
  const refresh = useCallback(() => { security.status().then(setS).catch(() => setS(null)); }, []);
  useEffect(refresh, [refresh]);
  useListen(() => security.onReady(refresh), [refresh]);
  return [s, refresh];
}
export type View = "active" | "archived" | "trash";
export function useConversations(view: View, query: string): [Conversation[] | null, () => void] {
  const [list, setList] = useState<Conversation[] | null>(null); const seq = useRef(0);
  const refresh = useCallback(() => { const n = ++seq.current; chat.list(view, query).then((l) => { if (n === seq.current) setList(l); }).catch(() => { if (n === seq.current) setList([]); }); }, [view, query]);
  useEffect(() => { const t = window.setTimeout(refresh, query ? 120 : 0); return () => window.clearTimeout(t); }, [refresh, query]);
  useListen(() => chat.onListChanged(refresh), [refresh]);
  useListen(() => security.onReady(refresh), [refresh]);
  return [list, refresh];
}
export function groupByDay(list: Conversation[], now = Date.now()): Array<[string, Conversation[]]> {
  const day = 86400; const t0 = new Date(now).setHours(0, 0, 0, 0) / 1000; const groups: Record<string, Conversation[]> = {}; const order = ["Today", "Yesterday", "Previous 7 days", "Earlier"];
  for (const c of list) { const g = c.updated >= t0 ? "Today" : c.updated >= t0 - day ? "Yesterday" : c.updated >= t0 - 7 * day ? "Previous 7 days" : "Earlier"; (groups[g] ??= []).push(c); }
  return order.filter((g) => groups[g]).map((g) => [g, groups[g]]);
}
