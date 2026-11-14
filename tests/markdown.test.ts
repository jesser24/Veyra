import { describe, expect, it } from "vitest";
import { parseBlocks, parseInline } from "../src/lib/markdown";

describe("markdown", () => {
  it("parses fenced code with language and keeps content verbatim (no inline parsing inside)", () => {
    const b = parseBlocks("Intro\n```rust\nlet a = **x**;\n```\nOutro");
    expect(b.map((x) => x.t)).toEqual(["p", "code", "p"]);
    expect(b[1]).toEqual({ t: "code", lang: "rust", v: "let a = **x**;", open: false });
  });
  it("treats an unterminated fence (a reply still streaming) as an open code block", () => {
    const b = parseBlocks("```py\nprint(1)"); expect(b).toEqual([{ t: "code", lang: "py", v: "print(1)", open: true }]);
  });
  it("parses headings, lists, quotes and paragraphs", () => {
    const b = parseBlocks("# Title\n\n- a\n- b\n\n1. one\n2. two\n\n> quoted\n\ntext");
    expect(b.map((x) => x.t)).toEqual(["h", "ul", "ol", "quote", "p"]);
    expect((b[1] as { items: unknown[] }).items).toHaveLength(2);
  });
  it("parses inline code, bold and italic; unmatched markers stay literal", () => {
    expect(parseInline("a `b` **c** *d*").map((x) => x.t)).toEqual(["text", "code", "text", "b", "text", "i"]);
    expect(parseInline("2 * 3 = 6")).toEqual([{ t: "text", v: "2 * 3 = 6" }]);
    expect(parseInline("**open")).toEqual([{ t: "text", v: "**open" }]);
  });
  it("never produces HTML: markup in model output stays text", () => {
    const b = parseBlocks("<script>alert(1)</script> and <img src=x onerror=1>"); expect(b).toEqual([{ t: "p", c: [{ t: "text", v: "<script>alert(1)</script> and <img src=x onerror=1>" }] }]);
  });
  it("parses GFM tables with alignment, inline formatting in cells and short rows", () => {
    const b = parseBlocks("Before\n\n| Name | Qty | Note |\n|:--|--:|:-:|\n| **ax** | 2 | `ok` |\n| bee | 4 |\n\nAfter");
    expect(b.map((x) => x.t)).toEqual(["p", "table", "p"]);
    const t = b[1] as Extract<(typeof b)[number], { t: "table" }>;
    expect(t.align).toEqual(["left", "right", "center"]); expect(t.rows).toHaveLength(2); expect(t.rows[0][0][0].t).toBe("b"); expect(t.rows[1]).toHaveLength(3);
  });
  it("does not turn a lone pipe line or a streaming half-table into a table", () => {
    expect(parseBlocks("a | b").map((x) => x.t)).toEqual(["p"]);
    expect(parseBlocks("| a | b |").map((x) => x.t)).toEqual(["p"]);
  });
  it("supports escaped pipes in cells and horizontal rules", () => {
    const t = parseBlocks("| a |\n|---|\n| x \\| y |")[0] as { rows: Array<Array<Array<{ t: string; v?: string }>>> };
    expect(t.rows[0][0][0].v).toBe("x | y"); expect(parseBlocks("one\n\n---\n\ntwo").map((x) => x.t)).toEqual(["p", "hr", "p"]);
  });
});
