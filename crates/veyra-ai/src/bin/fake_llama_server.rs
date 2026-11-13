//! Test double for `llama-server`: same CLI flags and HTTP/SSE shape for the endpoints Veyra uses. It performs NO inference; it exists only so the
//! supervisor, streaming, cancellation and unload logic can be tested deterministically in CI. It is never shipped.
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
fn handle(mut s: TcpStream, key: String) {
    let mut r = BufReader::new(s.try_clone().unwrap()); let mut line = String::new(); r.read_line(&mut line).ok();
    let path = line.split_whitespace().nth(1).unwrap_or("").to_string(); let (mut len, mut auth_ok) = (0usize, false);
    loop { let mut h = String::new(); if r.read_line(&mut h).unwrap_or(0) == 0 || h.trim().is_empty() { break; } let l = h.to_lowercase();
        if let Some(v) = l.strip_prefix("content-length:") { len = v.trim().parse().unwrap_or(0); } if l.starts_with("authorization:") && h.contains(&format!("Bearer {key}")) { auth_ok = true; } }
    let mut body = vec![0u8; len]; r.read_exact(&mut body).ok();
    if path == "/health" { let _ = s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 15\r\nConnection: close\r\n\r\n{\"status\":\"ok\"}"); return; }
    if !auth_ok { let _ = s.write_all(b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"); return; }
    if path == "/props" { let _ = s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}"); return; }
    if path == "/tokenize" { let v: serde_json::Value = serde_json::from_slice(&body).unwrap_or_default(); let n = v["content"].as_str().unwrap_or("").split_whitespace().count();
        let out = serde_json::json!({ "tokens": vec![1; n] }).to_string(); let _ = write!(s, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", out.len(), out); return; }
    if path == "/v1/chat/completions" {
        let _ = s.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n");
        let words = ["Hello", " from", " the", " fake", " engine", ".", " Done", "!"]; let slow = std::env::var("FAKE_SLOW_MS").ok().and_then(|v| v.parse().ok()).unwrap_or(5u64);
        for (i, w) in words.iter().cycle().take(if body.windows(6).any(|x| x == b"\"long\"") { 400 } else { words.len() }).enumerate() {
            let ev = serde_json::json!({ "choices": [{ "delta": { "content": w } }] }); if s.write_all(format!("data: {ev}\n\n").as_bytes()).is_err() { eprintln!("client went away after {i} tokens"); return; }
            let _ = s.flush(); std::thread::sleep(std::time::Duration::from_millis(slow)); }
        let fin = serde_json::json!({ "choices": [{ "delta": {} }], "timings": { "prompt_n": 7, "predicted_n": 8, "prompt_per_second": 100.0, "predicted_per_second": 12.5 } });
        let _ = s.write_all(format!("data: {fin}\n\ndata: [DONE]\n\n").as_bytes()); return; }
    let _ = s.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
}
fn main() {
    let a: Vec<String> = std::env::args().collect(); let get = |f: &str| a.iter().position(|x| x == f).and_then(|i| a.get(i + 1)).cloned();
    let model = get("--model").expect("--model"); if std::fs::metadata(&model).map(|m| m.len() == 0).unwrap_or(true) { eprintln!("failed to load model {model}"); std::process::exit(1); }
    let (port, key) = (get("--port").expect("--port"), get("--api-key").expect("--api-key"));
    if let Some(ms) = std::env::var("FAKE_LOAD_MS").ok().and_then(|v| v.parse().ok()) { std::thread::sleep(std::time::Duration::from_millis(ms)); }
    let l = TcpListener::bind(format!("127.0.0.1:{port}")).expect("bind");
    for c in l.incoming().flatten() { let k = key.clone(); std::thread::spawn(move || handle(c, k)); }
}
