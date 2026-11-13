use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::AtomicBool;
use std::time::Duration;
use veyra_ai::catalog::{ModelSpec, Status};
use veyra_ai::download::*;
use veyra_net::{Config, NetClient};

fn spec(data: &[u8], hash: &str) -> &'static ModelSpec {
    let h: &'static str = Box::leak(hash.to_string().into_boxed_str());
    Box::leak(Box::new(ModelSpec { id: "t", name: "t", file: "t.gguf", url: "", sha256: Some(h), size_gb: data.len() as f64 / 1_073_741_824.0, params_b: 1.0, quant: "q", license: "x", official_gguf: true, status: Status::Verified, min_ram_gb: 1.0, notes: "" }))
}
fn sha(d: &[u8]) -> String { use sha2::{Digest, Sha256}; Sha256::digest(d).iter().map(|b| format!("{b:02x}")).collect() }
/// Minimal HTTP server over loopback supporting Range; `cut_at` makes the first full response die half way.
fn serve(data: Vec<u8>, cut_first_at: Option<usize>) -> String {
    let l = TcpListener::bind("127.0.0.1:0").unwrap(); let url = format!("http://{}/m.gguf", l.local_addr().unwrap());
    std::thread::spawn(move || { let mut first = true; for mut s in l.incoming().flatten() {
        let mut buf = [0u8; 2048]; let n = s.read(&mut buf).unwrap_or(0); let req = String::from_utf8_lossy(&buf[..n]).to_string();
        let start = req.lines().find_map(|l| l.to_lowercase().strip_prefix("range: bytes=").and_then(|r| r.trim_end_matches('-').parse::<usize>().ok()));
        let (code, from) = match start { Some(f) => ("206 Partial Content", f), None => ("200 OK", 0) }; let body = &data[from..];
        let extra = if from > 0 { format!("Content-Range: bytes {from}-{}/{}\r\n", data.len() - 1, data.len()) } else { String::new() };
        let _ = write!(s, "HTTP/1.1 {code}\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n", body.len());
        if first { if let Some(c) = cut_first_at { let _ = s.write_all(&body[..c]); first = false; continue; } } first = false; let _ = s.write_all(body); } });
    url
}
fn net() -> NetClient { NetClient::new_for_tests(Config { connect_timeout: Duration::from_secs(3), request_timeout: Duration::from_secs(30), user_agent: "t".into() }).unwrap() }

#[tokio::test] async fn interrupted_download_resumes_and_is_verified() {
    let data: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect(); let sp = spec(&data, &sha(&data)); let d = tempfile::tempdir().unwrap();
    let url = serve(data.clone(), Some(100_000)); let n = net(); let c = AtomicBool::new(false);
    assert!(download(&n, d.path(), sp, Some(&url), None, &c, |_| {}).await.is_err(), "connection cut mid-way");
    let part = std::fs::metadata(part_path(d.path(), sp)).unwrap().len(); assert!(part > 0 && part < data.len() as u64, "progress kept: {part}");
    let p = download(&n, d.path(), sp, Some(&url), None, &c, |_| {}).await.unwrap(); assert_eq!(std::fs::read(&p).unwrap(), data); assert!(!part_path(d.path(), sp).exists());
    assert!(verify_installed(d.path(), sp).await.unwrap());
}
#[tokio::test] async fn corrupted_download_is_rejected_and_deleted() {
    let data = vec![7u8; 50_000]; let sp = spec(&data, &sha(b"something else")); let d = tempfile::tempdir().unwrap(); let url = serve(data, None);
    let r = download(&net(), d.path(), sp, Some(&url), None, &AtomicBool::new(false), |_| {}).await; assert!(matches!(r, Err(DlError::BadHash { .. })));
    assert!(!final_path(d.path(), sp).exists() && !part_path(d.path(), sp).exists());
}
#[tokio::test] async fn insufficient_disk_space_is_refused_before_any_request() {
    let data = vec![1u8; 10]; let sp = Box::leak(Box::new(ModelSpec { size_gb: 5.0, ..spec(&data, &sha(&data)).clone() })); let d = tempfile::tempdir().unwrap();
    let n = net(); let r = download(&n, d.path(), sp, Some("http://127.0.0.1:1/x"), Some(1 << 30), &AtomicBool::new(false), |_| {}).await;
    assert!(matches!(r, Err(DlError::Disk { .. }))); assert_eq!(n.ledger.summary().requests, 0);
}
#[tokio::test] async fn cancel_keeps_progress_and_already_installed_model_is_not_redownloaded() {
    let data: Vec<u8> = (0..200_000u32).map(|i| i as u8).collect(); let sp = spec(&data, &sha(&data)); let d = tempfile::tempdir().unwrap(); let url = serve(data.clone(), None);
    let n = net(); let r = download(&n, d.path(), sp, Some(&url), None, &AtomicBool::new(true), |_| {}).await; assert!(matches!(r, Err(DlError::Cancelled)));
    download(&n, d.path(), sp, Some(&url), None, &AtomicBool::new(false), |_| {}).await.unwrap(); let before = n.ledger.summary().requests;
    download(&n, d.path(), sp, Some(&url), None, &AtomicBool::new(false), |_| {}).await.unwrap(); assert_eq!(n.ledger.summary().requests, before);
}
