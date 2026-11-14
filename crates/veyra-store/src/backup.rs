//! Encrypted backups, independent from the live database. A backup is a folder containing the vault (wrapped keys), a snapshot of the
//! encrypted database, the encrypted attachment files and a manifest of SHA-256 sums. Restoring needs the password or the recovery key.
use crate::crypto::sha256_hex;
use crate::db::Store;
use crate::{vault, Error, Result};
use serde::{Deserialize, Serialize};
use std::{fs, path::{Path, PathBuf}};

#[derive(Serialize, Deserialize, Debug)]
struct Manifest { version: u32, created: i64, files: Vec<(String, String, u64)> } // (relative path, sha256, size)

fn collect(dir: &Path, base: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    if !dir.exists() { return Ok(()); }
    for e in fs::read_dir(dir)? { let p = e?.path(); if p.is_dir() { collect(&p, base, out)?; } else { out.push(p.strip_prefix(base).unwrap().to_path_buf()); } }
    Ok(())
}

/// `data_dir` holds `veyra.db` and `attachments/` (and, only for data written by early development builds, a legacy `vault.json`). Returns the new backup folder.
/// The data key is NOT in a backup: it lives in the operating system's credential store (see `system_key`); a backup is useless without it.
pub fn create(store: &Store, data_dir: &Path, backups_dir: &Path, now: i64) -> Result<PathBuf> {
    let dest = backups_dir.join(format!("veyra-backup-{now}"));
    if dest.exists() { return Err(Error::Format("a backup with this timestamp already exists".into())); }
    let tmp = backups_dir.join(format!(".tmp-{now}"));
    fs::create_dir_all(tmp.join("attachments"))?;
    let legacy = data_dir.join("vault.json").is_file(); if legacy { fs::copy(data_dir.join("vault.json"), tmp.join("vault.json"))?; }
    store.snapshot_to(&tmp.join("veyra.db"))?;
    let mut rels = vec![]; collect(&Store::attachments_dir(data_dir), data_dir, &mut rels)?;
    for r in &rels { fs::copy(data_dir.join(r), tmp.join(r))?; }
    let mut files = vec![];
    for r in legacy.then(|| "vault.json".to_string()).into_iter().chain(["veyra.db".to_string()]).chain(std::iter::empty()).chain(rels.iter().map(|p: &PathBuf| p.to_string_lossy().replace('\\', "/"))) {
        let b = fs::read(tmp.join(&r))?; files.push((r, sha256_hex(&b), b.len() as u64));
    }
    fs::write(tmp.join("MANIFEST.json"), serde_json::to_vec_pretty(&Manifest { version: 2, created: now, files }).unwrap())?;
    fs::rename(&tmp, &dest)?;
    Ok(dest)
}

/// Check every file of a backup against its manifest.
pub fn verify(backup: &Path) -> Result<()> {
    let m: Manifest = serde_json::from_slice(&fs::read(backup.join("MANIFEST.json")).map_err(|_| Error::Integrity("backup has no manifest".into()))?).map_err(|e| Error::Integrity(e.to_string()))?;
    for (rel, sum, size) in &m.files {
        let b = fs::read(backup.join(rel)).map_err(|_| Error::Integrity(format!("backup file missing: {rel}")))?;
        if b.len() as u64 != *size || &sha256_hex(&b) != sum { return Err(Error::Integrity(format!("backup file damaged: {rel}"))); }
    }
    Ok(())
}

/// Restore into an EMPTY `target_dir` (never overwrites existing data). Proves the secret works and the database opens and passes its
/// integrity check before reporting success; on any failure the target is cleaned up.
pub fn restore(backup: &Path, target_dir: &Path, password: Option<&str>, recovery: Option<&str>) -> Result<()> {
    let u = match (password, recovery) { (Some(p), _) => vault::unlock(&backup.join("vault.json"), p)?, (None, Some(r)) => vault::unlock_with_recovery(&backup.join("vault.json"), r)?, _ => return Err(Error::WrongSecret) };
    restore_with_key(backup, target_dir, &u)
}
/// Same guarantees as `restore`, with the data key supplied by the caller (the OS-protected key of this computer, or one imported from a key export).
pub fn restore_with_key(backup: &Path, target_dir: &Path, u: &crate::vault::Unlocked) -> Result<()> {
    verify(backup)?;
    if target_dir.exists() && fs::read_dir(target_dir)?.next().is_some() { return Err(Error::Format("restore target is not empty; refusing to overwrite existing data".into())); }
    fs::create_dir_all(target_dir)?;
    let res = (|| -> Result<()> {
        let mut rels = vec![]; collect(backup, backup, &mut rels)?;
        for r in rels { if r.to_string_lossy() == "MANIFEST.json" { continue; } let to = target_dir.join(&r); if let Some(d) = to.parent() { fs::create_dir_all(d)?; } fs::copy(backup.join(&r), to)?; }
        Store::open(target_dir, u)?.integrity_check()
    })();
    if res.is_err() { let _ = fs::remove_dir_all(target_dir); }
    res
}

/// Retention: delete backups older than `max_age_days`, then keep at most `max_count`, then drop oldest until under `max_bytes`. Newest backup is never deleted.
pub fn prune(backups_dir: &Path, now: i64, max_age_days: u32, max_count: usize, max_bytes: u64) -> Result<Vec<PathBuf>> {
    fn size(p: &Path) -> u64 { fs::read_dir(p).map(|d| d.flatten().map(|e| { let p = e.path(); if p.is_dir() { size(&p) } else { e.metadata().map(|m| m.len()).unwrap_or(0) } }).sum()).unwrap_or(0) }
    let mut all: Vec<(i64, PathBuf)> = fs::read_dir(backups_dir)?.flatten().filter_map(|e| { let n = e.file_name().to_string_lossy().to_string(); n.strip_prefix("veyra-backup-")?.parse::<i64>().ok().map(|t| (t, e.path())) }).collect();
    all.sort_by_key(|x| std::cmp::Reverse(x.0));
    let mut removed = vec![];
    let cutoff = now - max_age_days as i64 * 86_400_000;
    let mut keep: Vec<(i64, PathBuf)> = vec![];
    for (i, (t, p)) in all.into_iter().enumerate() { if i > 0 && (*&t < cutoff || i >= max_count) { fs::remove_dir_all(&p)?; removed.push(p); } else { keep.push((t, p)); } }
    while keep.len() > 1 && keep.iter().map(|(_, p)| size(p)).sum::<u64>() > max_bytes { let (_, p) = keep.pop().unwrap(); fs::remove_dir_all(&p)?; removed.push(p); }
    Ok(removed)
}
