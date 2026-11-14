//! Turns an attached file into text the local model can read, plus a short human summary. Everything is in memory and bounded:
//! archives are NEVER written to disk (so path traversal has nothing to act on), entry names that try to escape are skipped,
//! decompression is capped per entry / in total / by ratio (zip-bombs), nothing is executed or opened with another program.
//! Images are previewed by the UI; this crate only reports their format and size (the local text models cannot see pixels).
use serde::Serialize;
use std::io::{Cursor, Read};

#[derive(Clone, Debug)]
pub struct Limits { pub max_file: u64, pub max_text_chars: usize, pub zip_max_entries: usize, pub zip_max_total: u64, pub zip_max_entry: u64, pub zip_read_files: usize, pub zip_read_bytes: usize, pub max_ratio: u64 }
impl Default for Limits { fn default() -> Self { Limits { max_file: 50 << 20, max_text_chars: 400_000, zip_max_entries: 5000, zip_max_total: 500 << 20, zip_max_entry: 20 << 20, zip_read_files: 25, zip_read_bytes: 400_000, max_ratio: 200 } } }

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)] #[serde(rename_all = "lowercase")]
pub enum Kind { Image, Pdf, Word, Sheet, Slides, Csv, Text, Code, Archive, Unsupported }
#[derive(Serialize, Clone, Debug)] pub struct Entry { pub path: String, pub size: u64, pub read: bool }
#[derive(Serialize, Clone, Debug)]
pub struct Extracted { pub kind: Kind, pub mime: String, pub text: String, pub truncated: bool, pub summary: String, pub notes: Vec<String>, pub image: Option<(usize, usize)>, pub entries: Vec<Entry> }

fn nm(r: Result<std::borrow::Cow<'_, str>, zip::result::ZipError>) -> String { r.map(|c| c.into_owned()).unwrap_or_default() }
fn ext(name: &str) -> String { name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default() }
const CODE: &[&str] = &["rs","py","js","jsx","ts","tsx","c","h","cpp","hpp","cc","cs","java","kt","go","rb","php","swift","sh","bash","ps1","bat","sql","html","htm","css","scss","vue","svelte","lua","r","scala","dart","toml","ini","cfg","conf","json","xml","yaml","yml","gradle","make","mk","cmake","dockerfile","gitignore","env","lock","svg","tex","log"];
const TEXT: &[&str] = &["txt","md","markdown","rst","text"];
const IMG: &[&str] = &["png","jpg","jpeg","webp","gif","bmp","svg"];

pub fn kind_of(name: &str) -> Kind {
    let e = ext(name);
    match e.as_str() { "pdf" => Kind::Pdf, "docx" => Kind::Word, "xlsx" | "xlsm" => Kind::Sheet, "pptx" => Kind::Slides, "csv" | "tsv" => Kind::Csv, "zip" | "7z" | "rar" => Kind::Archive,
        "svg" => Kind::Code, // an image for the preview, but its source is text and is read as such
        x if IMG.contains(&x) => Kind::Image, x if TEXT.contains(&x) => Kind::Text, x if CODE.contains(&x) => Kind::Code,
        _ => { let l = name.to_ascii_lowercase(); if matches!(l.as_str(), "makefile" | "dockerfile" | "readme" | "license") { Kind::Text } else { Kind::Unsupported } } }
}
pub fn mime_of(name: &str) -> &'static str {
    match ext(name).as_str() { "png" => "image/png", "jpg" | "jpeg" => "image/jpeg", "webp" => "image/webp", "gif" => "image/gif", "bmp" => "image/bmp", "svg" => "image/svg+xml", "pdf" => "application/pdf",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document", "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet", "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        "csv" => "text/csv", "json" => "application/json", "xml" => "application/xml", "yaml" | "yml" => "application/yaml", "md" | "markdown" => "text/markdown", "zip" => "application/zip", "7z" => "application/x-7z-compressed", "rar" => "application/vnd.rar", _ => "text/plain" }
}
fn is_text_bytes(b: &[u8]) -> bool { let s = &b[..b.len().min(8192)]; !s.contains(&0) && std::str::from_utf8(s).is_ok() || (!s.contains(&0) && s.iter().filter(|c| **c < 9).count() == 0) }
fn clip(mut t: String, max: usize) -> (String, bool) { if t.chars().count() > max { t = t.chars().take(max).collect(); (t, true) } else { (t, false) } }
fn lossy(b: &[u8]) -> String { let b = b.strip_prefix(&[0xEF, 0xBB, 0xBF][..]).unwrap_or(b); String::from_utf8_lossy(b).into_owned() }

pub fn analyze(name: &str, bytes: &[u8], lim: &Limits) -> Result<Extracted, String> {
    if bytes.len() as u64 > lim.max_file { return Err(format!("This file is larger than the {} MB limit.", lim.max_file >> 20)); }
    if bytes.is_empty() { return Err("This file is empty.".into()); }
    let kind = kind_of(name); let mime = mime_of(name).to_string(); let mut notes = vec![];
    let mut out = Extracted { kind, mime, text: String::new(), truncated: false, summary: String::new(), notes: vec![], image: None, entries: vec![] };
    // a guard so that a parser bug on a hostile file becomes an error message, never a crash
    let r = std::panic::catch_unwind(|| inner(name, bytes, lim, kind));
    match r { Ok(Ok(p)) => { let (t, tr) = clip(p.text, lim.max_text_chars); out.text = t; out.truncated = tr || p.truncated; out.summary = p.summary; notes.extend(p.notes); out.image = p.image; out.entries = p.entries; }
        Ok(Err(e)) => return Err(e), Err(_) => return Err("This file could not be read safely (the document appears to be malformed).".into()) }
    if out.truncated { notes.push(format!("Only the first {} characters are kept.", lim.max_text_chars)); }
    out.notes = notes; Ok(out)
}
struct Part { text: String, truncated: bool, summary: String, notes: Vec<String>, image: Option<(usize, usize)>, entries: Vec<Entry> }
fn part(text: String, summary: String) -> Part { Part { text, truncated: false, summary, notes: vec![], image: None, entries: vec![] } }

fn inner(name: &str, b: &[u8], lim: &Limits, kind: Kind) -> Result<Part, String> {
    match kind {
        Kind::Image => { let (w, h) = imagesize::blob_size(b).map(|s| (s.width, s.height)).map_err(|_| "This image file is damaged or its format is not recognised.".to_string())?;
            let mut p = part(String::new(), format!("Image {w}×{h}")); p.image = Some((w, h));
            p.notes.push("The image is stored and shown, but the local text models cannot see it, and Veyra has no OCR yet: its content is not available to the model.".into()); Ok(p) }
        Kind::Text | Kind::Code => { if !is_text_bytes(b) { return Err("This file does not look like text.".into()); } let t = lossy(b); let lines = t.lines().count(); let mut p = part(t, format!("{lines} lines"));
            if ext(name) == "svg" { p.image = imagesize::blob_size(b).ok().map(|s| (s.width, s.height)); p.notes.push("SVG is shown as a picture (scripts never run) and read as text.".into()); } Ok(p) }
        Kind::Csv => csv(b, if ext(name) == "tsv" { b'\t' } else { 0 }),
        Kind::Pdf => { let t = pdf_extract::extract_text_from_mem(b).map_err(|e| format!("This PDF could not be read: {e}"))?; let t = t.replace('\u{0c}', "\n\n"); let pages = b.windows(7).filter(|w| *w == b"/Type /" || false).count(); let _ = pages;
            if t.trim().is_empty() { let mut p = part(String::new(), "PDF without a text layer".into()); p.notes.push("No text could be extracted: this looks like a scanned PDF. Veyra has no OCR yet.".into()); Ok(p) } else { let words = t.split_whitespace().count(); Ok(part(t, format!("PDF, about {words} words"))) } }
        Kind::Word => docx(b, lim), Kind::Slides => pptx(b, lim), Kind::Sheet => xlsx(b),
        Kind::Archive => match ext(name).as_str() { "zip" => zip_archive(b, lim), "7z" => sevenz(b, lim), _ => Err("RAR archives are not supported: the format can only be unpacked with a proprietary tool. Extract it yourself and attach the files (or a ZIP).".into()) },
        Kind::Unsupported => Err(format!("The format of “{name}” is not supported yet.")),
    }
}

// ---------- CSV: parse + describe columns so the model reasons over facts, not over a wall of commas
fn split_csv(text: &str, d: u8) -> Vec<Vec<String>> {
    let d = d as char; let mut rows = vec![]; let mut row = vec![]; let mut cell = String::new(); let mut q = false; let mut it = text.chars().peekable();
    while let Some(c) = it.next() {
        if q { if c == '"' { if it.peek() == Some(&'"') { cell.push('"'); it.next(); } else { q = false; } } else { cell.push(c); } }
        else if c == '"' && cell.is_empty() { q = true; } else if c == d { row.push(std::mem::take(&mut cell)); }
        else if c == '\n' || c == '\r' { if c == '\r' && it.peek() == Some(&'\n') { it.next(); } row.push(std::mem::take(&mut cell)); if !(row.len() == 1 && row[0].is_empty()) { rows.push(std::mem::take(&mut row)); } else { row.clear(); } }
        else { cell.push(c); }
    }
    if !cell.is_empty() || !row.is_empty() { row.push(cell); rows.push(row); } rows
}
fn csv(b: &[u8], forced: u8) -> Result<Part, String> {
    if !is_text_bytes(b) { return Err("This file does not look like text.".into()); } let text = lossy(b);
    let first = text.lines().next().unwrap_or(""); let d = if forced != 0 { forced } else { [b',', b';', b'\t', b'|'].into_iter().max_by_key(|c| first.matches(*c as char).count()).unwrap_or(b',') };
    let rows = split_csv(&text, d); if rows.is_empty() { return Err("This CSV file is empty.".into()); }
    let head = &rows[0]; let n = rows.len() - 1; let mut s = format!("CSV table: {n} data rows × {} columns.\nColumns:\n", head.len());
    for (i, h) in head.iter().enumerate() {
        let vals: Vec<&str> = rows[1..].iter().filter_map(|r| r.get(i).map(|x| x.trim())).filter(|x| !x.is_empty()).collect();
        let nums: Vec<f64> = vals.iter().filter_map(|v| v.replace(',', ".").parse::<f64>().ok()).collect();
        if !vals.is_empty() && nums.len() * 10 >= vals.len() * 9 { let mn = nums.iter().cloned().fold(f64::MAX, f64::min); let mx = nums.iter().cloned().fold(f64::MIN, f64::max); let sum: f64 = nums.iter().sum();
            s += &format!("- {h}: numeric, {} values, min {mn}, max {mx}, mean {:.4}, sum {sum:.4}\n", nums.len(), sum / nums.len() as f64); }
        else { let mut u: Vec<&str> = vals.clone(); u.sort(); u.dedup(); s += &format!("- {h}: text, {} non-empty, {} distinct{}\n", vals.len(), u.len(), if u.len() <= 8 { format!(": {}", u.join(", ")) } else { String::new() }); }
    }
    s += "\nFirst rows:\n"; for r in rows.iter().take(25) { s += &format!("| {} |\n", r.join(" | ")); }
    let mut truncated = false; if rows.len() > 25 { s += &format!("\n(… {} more rows; full data follows)\n\n", rows.len() - 25); for r in rows.iter().skip(25).take(2000) { s += &format!("| {} |\n", r.join(" | ")); } truncated = rows.len() > 2025; }
    let mut p = part(s, format!("{n} rows × {} columns", head.len())); p.truncated = truncated; Ok(p)
}

// ---------- Office (OOXML = zip + xml)
fn read_entry(z: &mut zip::ZipArchive<Cursor<&[u8]>>, name: &str, cap: u64) -> Result<Vec<u8>, String> {
    let f = z.by_name(name).map_err(|_| format!("{name} not found: this is not a valid Office file"))?; let mut v = vec![]; f.take(cap).read_to_end(&mut v).map_err(|e| e.to_string())?; Ok(v)
}
fn xml_text(xml: &[u8], para: &[u8], cell: &[u8], row: &[u8], text: &[u8]) -> String {
    use quick_xml::{events::Event, Reader}; let mut r = Reader::from_reader(xml); let mut buf = vec![]; let mut out = String::new(); let mut in_t = false; let mut in_cell = false;
    loop { match r.read_event_into(&mut buf) {
        Ok(Event::Start(e)) => { let n = e.name(); if n.as_ref() == text { in_t = true; } else if n.as_ref() == cell { in_cell = true; } }
        Ok(Event::Empty(e)) => { let n = e.name(); if n.as_ref() == b"w:tab" { out.push('\t'); } else if n.as_ref() == b"w:br" { out.push('\n'); } }
        Ok(Event::End(e)) => { let n = e.name(); let n = n.as_ref(); if n == text { in_t = false; } else if n == cell { in_cell = false; out.push_str(" | "); } else if n == row { out.push('\n'); } else if n == para { out.push(if in_cell { ' ' } else { '\n' }); } }
        Ok(Event::Text(t)) => { if in_t { if let Ok(d) = t.decode() { if let Ok(u) = quick_xml::escape::unescape(&d) { out.push_str(&u); } } } }
        Ok(Event::Eof) | Err(_) => break, _ => {} } buf.clear(); }
    out
}
fn docx(b: &[u8], lim: &Limits) -> Result<Part, String> {
    let mut z = zip::ZipArchive::new(Cursor::new(b)).map_err(|_| "This is not a valid .docx file.".to_string())?;
    let xml = read_entry(&mut z, "word/document.xml", lim.zip_max_entry)?; let t = xml_text(&xml, b"w:p", b"w:tc", b"w:tr", b"w:t");
    let t = t.lines().map(|l| l.trim_end_matches(" | ").trim_end()).collect::<Vec<_>>().join("\n"); let words = t.split_whitespace().count(); Ok(part(t, format!("Word document, about {words} words")))
}
fn pptx(b: &[u8], lim: &Limits) -> Result<Part, String> {
    let mut z = zip::ZipArchive::new(Cursor::new(b)).map_err(|_| "This is not a valid .pptx file.".to_string())?;
    let mut names: Vec<(usize, String)> = (0..z.len()).filter_map(|i| z.by_index(i).ok().map(|f| nm(f.name()))).filter_map(|n| n.strip_prefix("ppt/slides/slide").and_then(|r| r.strip_suffix(".xml")).and_then(|d| d.parse().ok()).map(|k| (k, n))).collect(); names.sort();
    if names.is_empty() { return Err("No slides found in this .pptx file.".into()); } let mut out = String::new();
    for (k, n) in &names { let xml = read_entry(&mut z, n, lim.zip_max_entry)?; out += &format!("## Slide {k}\n{}\n", xml_text(&xml, b"a:p", b"\x01", b"\x01", b"a:t").trim()); }
    Ok(part(out, format!("{} slides", names.len())))
}
fn xlsx(b: &[u8]) -> Result<Part, String> {
    use calamine::{open_workbook_auto_from_rs, Reader}; let mut wb = open_workbook_auto_from_rs(Cursor::new(b)).map_err(|_| "This is not a valid spreadsheet file.".to_string())?;
    let mut out = String::new(); let mut truncated = false; let names = wb.sheet_names().to_vec();
    for n in &names { if let Ok(r) = wb.worksheet_range(n) { let (h, w) = r.get_size(); out += &format!("## Sheet “{n}” ({h} rows × {w} columns)\n");
        for (i, row) in r.rows().enumerate() { if i >= 1000 { truncated = true; out += "(… more rows omitted)\n"; break; } out += &format!("| {} |\n", row.iter().map(|c| c.to_string()).collect::<Vec<_>>().join(" | ")); } out.push('\n'); } }
    let mut p = part(out, format!("{} sheet(s)", names.len())); p.truncated = truncated; Ok(p)
}

// ---------- archives: list everything, read a bounded selection of text-like files, write nothing
fn relevance(path: &str) -> (u8, usize) { let l = path.to_ascii_lowercase(); let base = l.rsplit('/').next().unwrap_or(""); let top = if base.starts_with("readme") { 0 } else if matches!(base, "package.json" | "cargo.toml" | "pyproject.toml" | "go.mod" | "pom.xml" | "requirements.txt" | "makefile") { 1 } else { 2 }; (top, l.matches('/').count()) }
fn readable(path: &str) -> bool { matches!(kind_of(path), Kind::Text | Kind::Code | Kind::Csv) }
fn safe_name(n: &str) -> bool { !(n.starts_with('/') || n.starts_with('\\') || n.contains("..") || n.contains(':') || n.contains('\0')) }
fn summarise(entries: Vec<(String, u64, Option<Vec<u8>>)>, notes: &mut Vec<String>, skipped_unsafe: usize) -> (String, Vec<Entry>) {
    let mut text = String::from("Archive contents:\n"); let mut list = vec![];
    for (p, s, d) in &entries { text += &format!("- {p} ({s} bytes){}\n", if d.is_some() { "" } else { "" }); list.push(Entry { path: p.clone(), size: *s, read: d.is_some() }); }
    for (p, _, d) in &entries { if let Some(d) = d { text += &format!("\n===== {p} =====\n{}\n", lossy(d)); } }
    if skipped_unsafe > 0 { notes.push(format!("{skipped_unsafe} entr{} with an unsafe path (absolute or “..”) were ignored.", if skipped_unsafe == 1 { "y" } else { "ies" })); }
    (text, list)
}
fn zip_archive(b: &[u8], lim: &Limits) -> Result<Part, String> {
    let mut z = zip::ZipArchive::new(Cursor::new(b)).map_err(|_| "This is not a valid ZIP archive (or it is encrypted).".to_string())?;
    if z.len() > lim.zip_max_entries { return Err(format!("This archive has {} entries; the limit is {}.", z.len(), lim.zip_max_entries)); }
    let mut notes = vec![]; let mut total = 0u64; let mut metas: Vec<(String, u64, u64)> = vec![]; let mut unsafe_n = 0;
    for i in 0..z.len() { let f = z.by_index_raw(i).map_err(|e| e.to_string())?; if f.is_dir() { continue; } let n = nm(f.name());
        if !safe_name(&n) { unsafe_n += 1; continue; } total = total.saturating_add(f.size()); metas.push((n, f.size(), f.compressed_size())); }
    if total > lim.zip_max_total { return Err(format!("This archive would expand to {} MB; the limit is {} MB (protection against decompression bombs).", total >> 20, lim.zip_max_total >> 20)); }
    let mut order: Vec<usize> = (0..metas.len()).filter(|i| readable(&metas[*i].0)).collect(); order.sort_by_key(|i| (relevance(&metas[*i].0), metas[*i].1));
    let (mut read, mut bytes) = (0, 0usize); let mut data: std::collections::HashMap<usize, Vec<u8>> = Default::default();
    for i in order { let (n, size, comp) = metas[i].clone(); if read >= lim.zip_read_files || bytes >= lim.zip_read_bytes { break; }
        if size > lim.zip_max_entry || (comp > 0 && size / comp.max(1) > lim.max_ratio && size > (1 << 20)) { notes.push(format!("“{n}” was not read (too large or suspicious compression ratio).")); continue; }
        let Ok(f) = z.by_name(&n) else { continue }; let mut v = vec![]; if f.take(lim.zip_max_entry.min((lim.zip_read_bytes - bytes) as u64)).read_to_end(&mut v).is_err() || !is_text_bytes(&v) { continue; } bytes += v.len(); read += 1; data.insert(i, v); }
    let entries: Vec<_> = metas.iter().enumerate().map(|(i, m)| (m.0.clone(), m.1, data.remove(&i))).collect(); let n = entries.len(); let nread = entries.iter().filter(|e| e.2.is_some()).count();
    if n > nread { notes.push(format!("Listed {n} files; read the text of {nread} of them (README and manifest files first). Other files are listed only.")); }
    notes.push("Nothing was extracted to disk and nothing is ever run.".into());
    let (text, list) = summarise(entries, &mut notes, unsafe_n); let mut p = part(text, format!("ZIP archive, {n} files")); p.notes = notes; p.entries = list; Ok(p)
}
fn sevenz(b: &[u8], lim: &Limits) -> Result<Part, String> {
    use sevenz_rust2::{ArchiveReader, Password}; let mut r = ArchiveReader::new(Cursor::new(b), Password::empty()).map_err(|_| "This is not a valid 7z archive (or it is encrypted).".to_string())?;
    let files: Vec<(String, u64)> = r.archive().files.iter().filter(|f| !f.is_directory()).map(|f| (f.name().to_string(), f.size())).collect();
    if files.len() > lim.zip_max_entries { return Err(format!("This archive has {} entries; the limit is {}.", files.len(), lim.zip_max_entries)); }
    let total: u64 = files.iter().map(|f| f.1).sum(); if total > lim.zip_max_total { return Err(format!("This archive would expand to {} MB; the limit is {} MB (protection against decompression bombs).", total >> 20, lim.zip_max_total >> 20)); }
    let mut notes = vec![]; let mut data: std::collections::HashMap<String, Vec<u8>> = Default::default(); let (mut read, mut bytes) = (0, 0usize); let mut unsafe_n = 0;
    let mut cands: Vec<&(String, u64)> = files.iter().filter(|f| safe_name(&f.0) && readable(&f.0) && f.1 <= lim.zip_max_entry).collect(); cands.sort_by_key(|f| (relevance(&f.0), f.1)); let want: std::collections::HashSet<String> = cands.iter().take(lim.zip_read_files).map(|f| f.0.clone()).collect();
    for f in &files { if !safe_name(&f.0) { unsafe_n += 1; } }
    // solid 7z blocks are decoded sequentially; only wanted files are kept, each capped
    let _ = r.for_each_entries(|e, rd| { if want.contains(e.name()) && read < lim.zip_read_files && bytes < lim.zip_read_bytes { let mut v = vec![]; let _ = rd.take(lim.zip_max_entry.min((lim.zip_read_bytes - bytes) as u64)).read_to_end(&mut v); if is_text_bytes(&v) { bytes += v.len(); read += 1; data.insert(e.name().to_string(), v); } } else { let mut sink = std::io::sink(); let _ = std::io::copy(&mut rd.take(lim.zip_max_entry), &mut sink); } Ok(true) });
    let entries: Vec<_> = files.iter().filter(|f| safe_name(&f.0)).map(|f| (f.0.clone(), f.1, data.remove(&f.0))).collect(); let n = entries.len(); notes.push("Nothing was extracted to disk and nothing is ever run.".into());
    let (text, list) = summarise(entries, &mut notes, unsafe_n); let mut p = part(text, format!("7z archive, {n} files")); p.notes = notes; p.entries = list; Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn zip_of(files: &[(&str, &[u8])]) -> Vec<u8> { use std::io::Write; let mut v = vec![]; { let mut w = zip::ZipWriter::new(Cursor::new(&mut v)); for (n, d) in files { w.start_file::<_, ()>(*n, zip::write::SimpleFileOptions::default()).unwrap(); w.write_all(d).unwrap(); } w.finish().unwrap(); } v }
    #[test] fn text_and_code_are_read() { let e = analyze("a.rs", b"fn main() {}\n", &Limits::default()).unwrap(); assert_eq!(e.kind, Kind::Code); assert!(e.text.contains("fn main")); }
    #[test] fn csv_gets_column_statistics() { let e = analyze("t.csv", b"name,qty\nax,2\nbee,4\n", &Limits::default()).unwrap(); assert!(e.text.contains("qty: numeric") && e.text.contains("mean 3")); assert!(e.text.contains("2 data rows")); }
    #[test] fn csv_handles_quotes_and_semicolons() { let e = analyze("t.csv", b"a;b\n\"x;y\";1\n", &Limits::default()).unwrap(); assert!(e.text.contains("x;y")); }
    #[test] fn zip_lists_and_reads_readme_first() { let z = zip_of(&[("src/a.rs", b"fn a(){}"), ("README.md", b"# hi"), ("img.png", &[0x89, 0, 1, 2])]); let e = analyze("p.zip", &z, &Limits::default()).unwrap(); assert_eq!(e.entries.len(), 3); assert!(e.text.contains("# hi") && e.text.contains("fn a")); assert!(!e.entries.iter().find(|x| x.path == "img.png").unwrap().read); }
    #[test] fn zip_path_traversal_entries_are_ignored_never_written() { let z = zip_of(&[("../evil.txt", b"x"), ("ok.txt", b"fine")]); let e = analyze("p.zip", &z, &Limits::default()).unwrap(); assert_eq!(e.entries.len(), 1); assert!(e.notes.iter().any(|n| n.contains("unsafe path"))); }
    #[test] fn zip_over_the_total_limit_is_refused() { let z = zip_of(&[("big.txt", &vec![b'a'; 4096])]); let lim = Limits { zip_max_total: 1000, ..Limits::default() }; assert!(analyze("p.zip", &z, &lim).unwrap_err().contains("decompression bombs")); }
    #[test] fn rar_is_refused_honestly() { assert!(analyze("a.rar", b"Rar!\x1a\x07\x00", &Limits::default()).unwrap_err().contains("not supported")); }
    #[test] fn oversize_file_is_refused() { let lim = Limits { max_file: 10, ..Limits::default() }; assert!(analyze("a.txt", &[b'a'; 20], &lim).is_err()); }
    #[test] fn garbage_office_files_fail_cleanly() { for n in ["a.docx", "a.xlsx", "a.pptx", "a.pdf", "a.zip", "a.7z", "a.png"] { assert!(analyze(n, b"not really a file at all", &Limits::default()).is_err(), "{n}"); } }
    #[test] fn docx_text_and_tables() { let xml = br#"<w:document><w:body><w:p><w:r><w:t>Hello</w:t></w:r></w:p><w:tbl><w:tr><w:tc><w:p><w:r><w:t>A</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>B</w:t></w:r></w:p></w:tc></w:tr></w:tbl></w:body></w:document>"#; let z = zip_of(&[("word/document.xml", xml)]); let e = analyze("d.docx", &z, &Limits::default()).unwrap(); assert!(e.text.contains("Hello") && e.text.contains("A") && e.text.contains("B")); }
    #[test] fn png_dimensions_and_no_vision_note() { let png: &[u8] = &[0x89,b'P',b'N',b'G',13,10,26,10,0,0,0,13,b'I',b'H',b'D',b'R',0,0,0,4,0,0,0,3,8,2,0,0,0,0,0,0,0]; let e = analyze("p.png", png, &Limits::default()).unwrap(); assert_eq!(e.image, Some((4, 3))); assert!(e.notes[0].contains("cannot see")); }
}
