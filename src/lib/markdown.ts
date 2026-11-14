/** Small, dependency-free Markdown parser producing a tree (rendered with React elements, never innerHTML → no script injection from model output).
 *  Supports: GFM tables, horizontal rules, fenced code (also unterminated while streaming), headings, bullet/numbered lists, paragraphs, blockquotes, **bold**, *italic*, `code`.
 *  Links are shown as text: Veyra never opens or loads remote content on its own. */
export type Inline = { t: "text"; v: string } | { t: "code"; v: string } | { t: "b"; c: Inline[] } | { t: "i"; c: Inline[] };
export type Block =
  | { t: "code"; lang: string; v: string; open: boolean } | { t: "h"; level: number; c: Inline[] } | { t: "p"; c: Inline[] }
  | { t: "ul"; items: Inline[][] } | { t: "ol"; items: Inline[][] } | { t: "quote"; c: Inline[] }
  | { t: "table"; head: Inline[][]; align: Array<"left" | "center" | "right" | null>; rows: Inline[][][] } | { t: "hr" };

export function parseInline(s: string): Inline[] {
  const out: Inline[] = []; let i = 0; let buf = "";
  const flush = () => { if (buf) { out.push({ t: "text", v: buf }); buf = ""; } };
  while (i < s.length) {
    if (s[i] === "`") { const j = s.indexOf("`", i + 1); if (j > i) { flush(); out.push({ t: "code", v: s.slice(i + 1, j) }); i = j + 1; continue; } }
    if (s.startsWith("**", i)) { const j = s.indexOf("**", i + 2); if (j > i + 2) { flush(); out.push({ t: "b", c: parseInline(s.slice(i + 2, j)) }); i = j + 2; continue; } }
    if (s[i] === "*" && s[i + 1] !== " " && s[i + 1] !== "*") { const j = s.indexOf("*", i + 1); if (j > i + 1 && s[j - 1] !== " ") { flush(); out.push({ t: "i", c: parseInline(s.slice(i + 1, j)) }); i = j + 1; continue; } }
    buf += s[i]; i++;
  }
  flush(); return out;
}

const splitRow = (l: string): string[] => { let t = l.trim(); if (t.startsWith("|")) t = t.slice(1); if (t.endsWith("|") && !t.endsWith("\\|")) t = t.slice(0, -1); return t.split(/(?<!\\)\|/).map((c) => c.trim().replace(/\\\|/g, "|")); };
const isSep = (l: string) => /^\s*\|?\s*:?-+:?\s*(\|\s*:?-+:?\s*)*\|?\s*$/.test(l) && l.includes("-") && (l.includes("|") || false);
const looksRow = (l: string) => l.includes("|") && l.trim().length > 1;

export function parseBlocks(src: string): Block[] {
  const lines = src.replace(/\r\n/g, "\n").split("\n"); const out: Block[] = []; let i = 0;
  const para: string[] = []; const endPara = () => { if (para.length) { out.push({ t: "p", c: parseInline(para.join("\n")) }); para.length = 0; } };
  while (i < lines.length) {
    const l = lines[i]; const fence = /^\s*```\s*([\w+#.-]*)\s*$/.exec(l);
    if (fence) { endPara(); const body: string[] = []; i++; let closed = false; while (i < lines.length) { if (/^\s*```\s*$/.test(lines[i])) { closed = true; i++; break; } body.push(lines[i]); i++; } out.push({ t: "code", lang: fence[1], v: body.join("\n"), open: !closed }); continue; }
    if (looksRow(l) && i + 1 < lines.length && isSep(lines[i + 1]) && splitRow(l).length === splitRow(lines[i + 1]).length) {
      endPara(); const head = splitRow(l); const align = splitRow(lines[i + 1]).map((c) => (c.startsWith(":") && c.endsWith(":") ? "center" : c.endsWith(":") ? "right" : c.startsWith(":") ? "left" : null)) as Array<"left" | "center" | "right" | null>;
      i += 2; const rows: Inline[][][] = []; while (i < lines.length && looksRow(lines[i]) && lines[i].trim() !== "") { const r = splitRow(lines[i]); rows.push(head.map((_, k) => parseInline(r[k] ?? ""))); i++; }
      out.push({ t: "table", head: head.map((h) => parseInline(h)), align, rows }); continue;
    }
    if (/^\s*([-*_])(\s*\1){2,}\s*$/.test(l) && para.length === 0) { out.push({ t: "hr" }); i++; continue; }
    const h = /^(#{1,4})\s+(.*)$/.exec(l); if (h) { endPara(); out.push({ t: "h", level: h[1].length, c: parseInline(h[2]) }); i++; continue; }
    if (/^\s*[-*+]\s+/.test(l)) { endPara(); const items: Inline[][] = []; while (i < lines.length && /^\s*[-*+]\s+/.test(lines[i])) { items.push(parseInline(lines[i].replace(/^\s*[-*+]\s+/, ""))); i++; } out.push({ t: "ul", items }); continue; }
    if (/^\s*\d+[.)]\s+/.test(l)) { endPara(); const items: Inline[][] = []; while (i < lines.length && /^\s*\d+[.)]\s+/.test(lines[i])) { items.push(parseInline(lines[i].replace(/^\s*\d+[.)]\s+/, ""))); i++; } out.push({ t: "ol", items }); continue; }
    if (/^\s*>\s?/.test(l)) { endPara(); const q: string[] = []; while (i < lines.length && /^\s*>\s?/.test(lines[i])) { q.push(lines[i].replace(/^\s*>\s?/, "")); i++; } out.push({ t: "quote", c: parseInline(q.join("\n")) }); continue; }
    if (l.trim() === "") { endPara(); i++; continue; }
    para.push(l); i++;
  }
  endPara(); return out;
}
