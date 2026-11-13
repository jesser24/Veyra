//! What is on disk and how far it can be trusted. A model is only "verified" if a full SHA-256 check succeeded for exactly the file that is
//! there now (size + modification time recorded in `verified.json` next to the models). Replacing or truncating the file invalidates it.
//! Loading requires "verified": a file merely existing on disk never makes a model usable.
use crate::catalog::ModelSpec;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
struct Mark { size: u64, mtime: u64, sha256: String }
type Marks = BTreeMap<String, Mark>;

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)] #[serde(rename_all = "snake_case")]
pub enum FileState { Available, Partial, Downloaded, Verified }

fn stat(p: &Path) -> Option<(u64, u64)> { let m = std::fs::metadata(p).ok()?; let t = m.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs(); Some((m.len(), t)) }
fn read(dir: &Path) -> Marks { std::fs::read_to_string(dir.join("verified.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default() }
fn write(dir: &Path, m: &Marks) -> std::io::Result<()> { let tmp = dir.join("verified.json.tmp"); std::fs::write(&tmp, serde_json::to_vec_pretty(m).unwrap())?; std::fs::rename(tmp, dir.join("verified.json")) }

pub fn state(dir: &Path, spec: &ModelSpec) -> (FileState, u64) {
    if spec.file.is_empty() { return (FileState::Available, 0); }
    if let Some((size, mtime)) = stat(&dir.join(spec.file)) {
        let ok = read(dir).get(spec.id).map(|m| m.size == size && m.mtime == mtime && Some(m.sha256.as_str()) == spec.sha256).unwrap_or(false);
        return (if ok { FileState::Verified } else { FileState::Downloaded }, size);
    }
    match stat(&dir.join(format!("{}.part", spec.file))) { Some((n, _)) => (FileState::Partial, n), None => (FileState::Available, 0) }
}
/// Call only right after a successful full-file hash comparison.
pub fn mark_verified(dir: &Path, spec: &ModelSpec) -> std::io::Result<()> {
    let (size, mtime) = stat(&dir.join(spec.file)).ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "model file missing"))?;
    let mut m = read(dir); m.insert(spec.id.into(), Mark { size, mtime, sha256: spec.sha256.unwrap_or_default().into() }); write(dir, &m)
}
pub fn remove(dir: &Path, spec: &ModelSpec) -> std::io::Result<()> {
    for f in [spec.file.to_string(), format!("{}.part", spec.file)] { let p = dir.join(f); if p.exists() { std::fs::remove_file(p)?; } }
    let mut m = read(dir); if m.remove(spec.id).is_some() { write(dir, &m)?; } Ok(())
}

#[cfg(test)]
mod tests {
    use super::*; use crate::catalog::{ModelSpec, Status};
    fn spec() -> ModelSpec { ModelSpec { id: "t", name: "t", file: "t.gguf", url: "", sha256: Some("ab"), size_gb: 0.0, params_b: 1.0, quant: "q", license: "x", official_gguf: true, status: Status::Verified, min_ram_gb: 1.0, notes: "" } }
    #[test] fn a_file_on_disk_is_not_usable_until_verified_and_modification_invalidates_the_mark() {
        let d = tempfile::tempdir().unwrap(); let s = spec(); assert_eq!(state(d.path(), &s).0, FileState::Available);
        std::fs::write(d.path().join("t.gguf.part"), b"12").unwrap(); assert_eq!(state(d.path(), &s), (FileState::Partial, 2));
        std::fs::write(d.path().join("t.gguf"), b"hello").unwrap(); assert_eq!(state(d.path(), &s).0, FileState::Downloaded, "exists but NOT verified");
        mark_verified(d.path(), &s).unwrap(); assert_eq!(state(d.path(), &s).0, FileState::Verified);
        std::fs::write(d.path().join("t.gguf"), b"hello world").unwrap(); assert_eq!(state(d.path(), &s).0, FileState::Downloaded, "changed file loses its mark");
        mark_verified(d.path(), &s).unwrap(); remove(d.path(), &s).unwrap(); assert_eq!(state(d.path(), &s).0, FileState::Available);
    }
    #[test] fn a_mark_for_another_hash_is_ignored() {
        let d = tempfile::tempdir().unwrap(); let s = spec(); std::fs::write(d.path().join("t.gguf"), b"x").unwrap(); mark_verified(d.path(), &s).unwrap();
        let mut other = spec(); other.sha256 = Some("cd"); assert_eq!(state(d.path(), &other).0, FileState::Downloaded);
    }
}
