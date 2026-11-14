import { describe, expect, it } from "vitest";
import { groupByDay } from "../src/lib/hooks";
import type { Conversation } from "../src/lib/ai";

const c = (id: string, updated: number): Conversation => ({ id, title: id, archived: false, favorite: false, deleted_at: null, created: updated, updated, summary: null });
describe("history grouping", () => {
  it("groups by recency and keeps the given order inside a group", () => {
    const now = new Date(2026, 10, 14, 15, 0, 0).getTime(); const s = (d: number, h = 12) => new Date(2026, 10, 14 - d, h, 0, 0).getTime() / 1000;
    const g = groupByDay([c("a", s(0)), c("b", s(0, 9)), c("y", s(1)), c("w", s(4)), c("old", s(30))], now);
    expect(g.map(([n]) => n)).toEqual(["Today", "Yesterday", "Previous 7 days", "Earlier"]); expect(g[0][1].map((x) => x.id)).toEqual(["a", "b"]);
  });
  it("omits empty groups", () => { expect(groupByDay([], Date.now())).toEqual([]); });
});
