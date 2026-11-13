//! Verified, resumable model download through the central network gateway (purpose `ModelDownload`, always user-initiated).
//! `<file>.part` holds the bytes received so far; a restart resumes with an HTTP Range request. Integrity = SHA-256 of the whole file against the
//! catalog hash, checked before the atomic rename to the final name. A file that fails verification is deleted, never loaded.
use crate::catalog::ModelSpec;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use veyra_net::{Initiator, NetClient, NetError, Purpose};

#[derive(Debug)]
pub enum DlError { Disk { need: u64, free: u64 }, Net(NetError), Io(String), BadHash { expected: String, got: String }, Cancelled, NoHash }
impl std::fmt::Display for DlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self { DlError::Disk { need, free } => write!(f, "not enough disk space: {} MiB needed, {} MiB free", need >> 20, free >> 20),
            DlError::Net(e) => write!(f, "network: {e:?}"), DlError::Io(e) => write!(f, "disk error: {e}"),
            DlError::BadHash { expected, got } => write!(f, "integrity check failed (expected {expected}, got {got}); file deleted"),
            DlError::Cancelled => write!(f, "download cancelled (progress kept, can be resumed)"), DlError::NoHash => write!(f, "model has no verified hash; refusing to download") }
    }
}
impl From<std::io::Error> for DlError { fn from(e: std::io::Error) -> Self { DlError::Io(e.to_string()) } }
impl From<NetError> for DlError { fn from(e: NetError) -> Self { DlError::Net(e) } }

#[derive(Clone, Debug, serde::Serialize)]
pub struct Progress { pub phase: &'static str, pub done: u64, pub total: Option<u64> }

pub fn final_path(dir: &Path, spec: &ModelSpec) -> PathBuf { dir.join(spec.file) }
pub fn part_path(dir: &Path, spec: &ModelSpec) -> PathBuf { dir.join(format!("{}.part", spec.file)) }

pub async fn sha256_file(p: &Path, mut on: impl FnMut(u64)) -> std::io::Result<String> {
    let mut f = tokio::fs::File::open(p).await?; let mut h = Sha256::new(); let mut buf = vec![0u8; 1 << 20]; let mut n = 0u64;
    loop { let r = f.read(&mut buf).await?; if r == 0 { break; } h.update(&buf[..r]); n += r as u64; on(n); }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// Is the model present and intact? (full hash; ~seconds for a few GB). Cheap check: `final_path(..).exists()`.
pub async fn verify_installed(dir: &Path, spec: &ModelSpec) -> Result<bool, DlError> {
    let Some(exp) = spec.sha256 else { return Err(DlError::NoHash) };
    let p = final_path(dir, spec); if !p.exists() { return Ok(false); }
    Ok(sha256_file(&p, |_| {}).await? == exp)
}

/// `url_override` exists for tests (a local HTTP server); production passes `None` and the catalog URL is used.
pub async fn download(net: &NetClient, dir: &Path, spec: &ModelSpec, url_override: Option<&str>, free_bytes: Option<u64>, cancel: &AtomicBool, mut progress: impl FnMut(Progress)) -> Result<PathBuf, DlError> {
    let exp = spec.sha256.ok_or(DlError::NoHash)?; tokio::fs::create_dir_all(dir).await?;
    let fin = final_path(dir, spec); let part = part_path(dir, spec);
    if fin.exists() { progress(Progress { phase: "verifying", done: 0, total: None }); if sha256_file(&fin, |_| {}).await? == exp { return Ok(fin); } tokio::fs::remove_file(&fin).await?; }
    let have = tokio::fs::metadata(&part).await.map(|m| m.len()).unwrap_or(0);
    let expected_total = (spec.size_gb * 1_073_741_824.0) as u64;
    if let Some(free) = free_bytes { let need = expected_total.saturating_sub(have) + (256 << 20); if free < need { return Err(DlError::Disk { need, free }); } }
    let url = url_override.unwrap_or(spec.url);
    let hdrs: Vec<(&str, String)> = if have > 0 { vec![("Range", format!("bytes={have}-"))] } else { vec![] };
    let mut resp = net.request(Purpose::ModelDownload, Initiator::User, reqwest::Method::GET, url, &hdrs, None, Some(std::time::Duration::from_secs(6 * 3600))).await?;
    let (mut done, total) = match resp.status() {
        206 => (have, resp.header("content-range").and_then(|v| v.rsplit('/').next().and_then(|t| t.parse().ok()))),
        200 => (0, resp.header("content-length").and_then(|v| v.parse().ok())), // server ignored Range: start over
        416 => { // our .part is not a valid prefix (or already complete): restart cleanly
            let _ = tokio::fs::remove_file(&part).await; resp.finish(None);
            return Err(DlError::Io("partial file was inconsistent and was removed; retry".into())); }
        s => { resp.finish(Some(veyra_net::ErrorClass::Http)); return Err(DlError::Net(NetError::Status(s))); }
    };
    let mut f = if done == 0 { tokio::fs::File::create(&part).await? } else { tokio::fs::OpenOptions::new().append(true).open(&part).await? };
    let mut last = std::time::Instant::now();
    loop {
        if cancel.load(Ordering::Relaxed) { f.flush().await?; resp.finish(Some(veyra_net::ErrorClass::Cancelled)); return Err(DlError::Cancelled); }
        match resp.chunk().await { Ok(Some(c)) => { f.write_all(&c).await?; done += c.len() as u64; if last.elapsed().as_millis() > 150 { progress(Progress { phase: "downloading", done, total }); last = std::time::Instant::now(); } }
            Ok(None) => break, Err(e) => { f.flush().await?; return Err(e.into()); } } // bytes already written stay on disk for resume
    }
    f.flush().await?; f.sync_all().await?; drop(f);
    progress(Progress { phase: "verifying", done: 0, total: Some(done) });
    let got = sha256_file(&part, |n| { let _ = n; }).await?;
    if got != exp { let _ = tokio::fs::remove_file(&part).await; return Err(DlError::BadHash { expected: exp.into(), got }); }
    tokio::fs::rename(&part, &fin).await?; progress(Progress { phase: "done", done, total: Some(done) });
    Ok(fin)
}
