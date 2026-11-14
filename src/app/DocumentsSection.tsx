import { Card, Icon, type IconName } from "../components/ui";

const ROWS: Array<[IconName, string, string, string]> = [
  ["image", "Images", "PNG, JPG, WebP, GIF, BMP, SVG", "Shown as a preview. The local models are text-only and Veyra has no OCR yet, so the content of a picture is NOT available to the answer. SVG is drawn as a picture (scripts never run) and its source is read as text."],
  ["doc", "PDF", "PDF", "Text layer extracted locally. Scanned PDFs without a text layer give no text (no OCR yet)."],
  ["doc", "Word & presentations", "DOCX, PPTX", "Paragraphs, tables and slide text, in reading order. Layout, comments and images are not read."],
  ["layers", "Spreadsheets & tables", "XLSX, CSV, TSV", "Sheets as tables; CSV also gets a column-by-column summary (types, ranges, averages) before the data."],
  ["file", "Text, data & code", "TXT, Markdown, JSON, XML, YAML, TOML, HTML and most source languages", "Read as text."],
  ["zip", "Archives", "ZIP, 7z", "Listed, then the text-like files are read (README and manifests first), in memory only: nothing is written to disk and nothing is ever run. Entries with unsafe paths are ignored; size, count and compression-ratio limits stop decompression bombs. Encrypted archives are not supported."],
  ["zip", "RAR", "RAR", "Not supported: the format can only be unpacked with a proprietary tool. Extract it yourself, or attach a ZIP."],
];
export function DocumentsSection() {
  return (
    <div className="stack">
      <Card title="What you can attach" hint="Add files with the paperclip, by drag and drop, or by pasting an image. Everything is read on this computer.">
        <ul className="rows info">{ROWS.map(([ic, t, ext, d]) => <li key={t}><span className="emblem"><Icon name={ic} size={18} /></span><div className="row-main"><b>{t} <small>{ext}</small></b><p className="hint">{d}</p></div></li>)}</ul>
      </Card>
      <Card title="Limits and storage">
        <dl className="facts"><dt>Per file</dt><dd>50 MB</dd><dt>Per message</dt><dd>10 files</dd><dt>Archive</dt><dd>5 000 entries, 500 MB once expanded, 25 files read</dd>
          <dt>Stored</dt><dd>With the conversation, encrypted on disk. Deleting the conversation deletes its files.</dd>
          <dt>Sent to the model</dt><dd>The extracted text, trimmed to fit the context window (the newest question may use about half of it). Long files are cut, and the model is told.</dd></dl>
      </Card>
    </div>
  );
}
