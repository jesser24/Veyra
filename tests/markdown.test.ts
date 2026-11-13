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
});
