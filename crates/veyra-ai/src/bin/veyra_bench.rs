//! Reproducible end-to-end benchmark and real-inference check. Runs the REAL llama-server with a REAL model:
//!   veyra-bench --server <llama-server> --dir <models dir> [--model qwen3-4b-q5_k_m] [--ctx 8192] [--threads N] [--runs 3] [--long] [--check] [--out report.json]
//! Downloads the model through Veyra's own gateway if missing (SHA-256 verified), then measures: download, load, first token, tokens/s, RAM, CPU,
//! cancellation latency, long-prompt behaviour, repeated generations, unload + reload. Nothing is estimated: every figure comes from this run.
use serde_json::json;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use sysinfo::{Pid, ProcessesToUpdate, System};
use veyra_ai::{catalog, context::Msg, download, engine::*, hardware};
use veyra_net::{Config, NetClient};

fn arg(a: &[String], k: &str) -> Option<String> { a.iter().position(|x| x == k).and_then(|i| a.get(i + 1)).cloned() }
fn user(s: &str) -> Vec<Msg> { vec![Msg { role: "user".into(), content: s.into() }] }

/// Average CPU use (percent of ONE core, so 400 = four cores busy) of the engine process while `f` runs.
async fn with_cpu<T>(pid: u32, f: impl std::future::Future<Output = T>) -> (T, f32) {
    let stop = Arc::new(AtomicBool::new(false)); let s2 = stop.clone();
    let h = tokio::task::spawn_blocking(move || { let mut sys = System::new(); let p = Pid::from_u32(pid); let mut v = vec![]; sys.refresh_processes(ProcessesToUpdate::Some(&[p]), true);
        while !s2.load(Ordering::Relaxed) { std::thread::sleep(Duration::from_millis(500)); sys.refresh_processes(ProcessesToUpdate::Some(&[p]), true); if let Some(pr) = sys.process(p) { v.push(pr.cpu_usage()); } } if v.is_empty() { 0.0 } else { v.iter().sum::<f32>() / v.len() as f32 } });
    let r = f.await; stop.store(true, Ordering::Relaxed); (r, h.await.unwrap_or(0.0))
}
fn rss(pid: u32) -> Option<u64> { let mut s = System::new(); let p = Pid::from_u32(pid); s.refresh_processes(ProcessesToUpdate::Some(&[p]), true); s.process(p).map(|p| p.memory() / 1_048_576) }

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap();
    let code = rt.block_on(run(a)); std::process::exit(code);
}

async fn run(a: Vec<String>) -> i32 {
    let id = arg(&a, "--model").unwrap_or_else(|| "qwen3-0.6b-q8_0".into()); let spec = match catalog::by_id(&id) { Some(s) if s.status == catalog::Status::Verified => s, _ => { eprintln!("unknown or unverified model id {id}"); return 2 } };
    let server = PathBuf::from(arg(&a, "--server").or_else(|| std::env::var("VEYRA_LLAMA_SERVER").ok()).expect("--server <path to llama-server>"));
    let dir = PathBuf::from(arg(&a, "--dir").unwrap_or_else(|| "veyra-bench-models".into())); let runs: usize = arg(&a, "--runs").and_then(|v| v.parse().ok()).unwrap_or(3);
    let hw = hardware::detect(); let ctx: u32 = arg(&a, "--ctx").and_then(|v| v.parse().ok()).unwrap_or(if hw.available_ram_mib >= 6 * 1024 { 8192 } else { 4096 }); let threads: usize = arg(&a, "--threads").and_then(|v| v.parse().ok()).unwrap_or_else(|| hardware::auto_threads(&hw));
    let check = a.iter().any(|x| x == "--check"); let long = a.iter().any(|x| x == "--long"); let mut failures: Vec<String> = vec![];
    println!("== Veyra benchmark ==\nhardware: {} | {} cores/{} threads | {} MB RAM ({} MB free) | avx2={} | {}\nmodel: {} ({}) ctx={ctx} threads={threads}", hw.cpu_brand, hw.physical_cores, hw.logical_cores, hw.total_ram_mib, hw.available_ram_mib, hw.avx2, hw.os, spec.name, spec.quant);
    let net = NetClient::new(Config::default()).expect("net");
    // 1. download (or verify what is already there)
    let t = Instant::now(); let had = download::final_path(&dir, spec).exists(); let mut last = Instant::now();
    let cancel = AtomicBool::new(false);
    let path = match download::download(&net, &dir, spec, None, hardware::free_disk_bytes(&dir), &cancel, |p| { if last.elapsed() > Duration::from_secs(5) { println!("  {} {} / {:?}", p.phase, p.done >> 20, p.total.map(|t| t >> 20)); last = Instant::now(); } }).await {
        Ok(p) => p, Err(e) => { eprintln!("download/verify failed: {e}"); return 1 } };
    let dl_s = t.elapsed().as_secs_f64(); println!("download+SHA-256 verification: {dl_s:.1} s ({})", if had { "file was already present: this is hash time only" } else { "network + hash" });
    let ext = net.ledger.summary(); println!("network ledger: {} external request(s) to {:?}", ext.external_requests, ext.hosts);
    // 2. load
    let cfg = EngineConfig { server_bin: server.clone(), model: path.clone(), ctx, threads, load_timeout: Duration::from_secs(600) };
    let t = Instant::now(); let mut eng = match Engine::load(net.clone(), cfg.clone()).await { Ok(e) => e, Err(e) => { eprintln!("engine load failed: {e}"); return 1 } };
    let load_ms = t.elapsed().as_millis() as u64; let pid = eng.pid().unwrap_or(0); let rss_idle = rss(pid); println!("load: {load_ms} ms | RAM after load: {rss_idle:?} MB");
    let mut report = json!({ "hardware": hw, "model": spec.id, "ctx": ctx, "threads": threads, "download_verify_s": dl_s, "load_ms": load_ms, "rss_after_load_mb": rss_idle });
    // 3. repeated generations
    let mut gens = vec![]; let (mut tps, mut ttft) = (vec![], vec![]);
    for i in 0..runs {
        let mut pieces = 0usize; let ((r, cpu), _) = (with_cpu(pid, eng.chat(&user("Write two sentences about the sea. Do not use lists."), &GenParams { max_tokens: 96, temperature: 0.7, top_p: 0.8, thinking: false }, &AtomicBool::new(false), |_| pieces += 1, |_| {})).await, 0);
        match r { Ok((text, p)) => { println!("run {}: {} tokens | {} | first token {} ms | cpu {cpu:.0}% of one core | rss {:?} MB", i + 1, p.generated_tokens.unwrap_or(0), p.generated_tokens_per_s.map(|x| format!("{x:.1} tok/s")).unwrap_or("?".into()), p.first_token_ms.unwrap_or(0), rss(pid));
                if text.trim().is_empty() { failures.push(format!("run {}: empty answer", i + 1)); } if pieces < 3 { failures.push(format!("run {}: reply was not streamed ({pieces} pieces)", i + 1)); }
                if let Some(x) = p.generated_tokens_per_s { tps.push(x); } if let Some(x) = p.first_token_ms { ttft.push(x as f64); }
                gens.push(json!({ "text": text, "perf": p, "cpu_pct_of_one_core": cpu, "rss_mb": rss(pid) })); }
            Err(e) => { failures.push(format!("run {} failed: {e}", i + 1)); } }
    }
    let mean = |v: &Vec<f64>| if v.is_empty() { None } else { Some(v.iter().sum::<f64>() / v.len() as f64) };
    // sanity: arithmetic prompt, so "a real model answered" is checked on content, not only on length
    if let Ok((t, _)) = eng.chat(&user("What is 17 + 25? Answer with only the number."), &GenParams { max_tokens: 16, temperature: 0.0, top_p: 1.0, thinking: false }, &AtomicBool::new(false), |_| {}, |_| {}).await { println!("sanity (17+25): {:?}", t.trim()); if !t.contains("42") { failures.push(format!("sanity answer wrong: {t:?}")); } report["sanity_17_plus_25"] = json!(t); }
    // 4. cancellation latency
    let cancel = AtomicBool::new(false); let mut n = 0; let mut cancelled_at: Option<Instant> = None;
    let r = eng.chat(&user("Count from 1 to 400 separated by commas."), &GenParams { max_tokens: 800, temperature: 0.2, top_p: 0.9, thinking: false }, &cancel, |_| { n += 1; if n == 15 { cancelled_at = Some(Instant::now()); cancel.store(true, Ordering::Relaxed); } }, |_| {}).await;
    let cancel_ms = cancelled_at.map(|t| t.elapsed().as_millis() as u64); println!("cancel: result {:?}, returned {cancel_ms:?} ms after the stop request", r.as_ref().err().map(|e| e.to_string()));
    if !matches!(r, Err(EngineError::Cancelled)) || cancel_ms.map(|m| m > 3000).unwrap_or(true) { failures.push(format!("cancellation not prompt: {cancel_ms:?} ms")); }
    // 5. long prompt (--long): about 70% of the context in one message
    if long {
        let filler = "The quick brown fox jumps over the lazy dog while the committee reviews the quarterly figures. "; let words_per = filler.split_whitespace().count(); let target = (ctx as f64 * 0.7) as usize; let reps = target / (words_per + 3);
        let big = filler.repeat(reps) + "\nQuestion: what animal jumps over the dog? Answer in one word.";
        let (r, cpu) = with_cpu(pid, eng.chat(&user(&big), &GenParams { max_tokens: 16, temperature: 0.0, top_p: 1.0, thinking: false }, &AtomicBool::new(false), |_| {}, |_| {})).await;
        match r { Ok((t, p)) => { println!("long prompt ({} prompt tokens): first token {} ms | prompt {:?} tok/s | answer {:?} | cpu {cpu:.0}%", p.prompt_tokens.unwrap_or(0), p.first_token_ms.unwrap_or(0), p.prompt_tokens_per_s, t.trim()); report["long_prompt"] = json!({ "perf": p, "answer": t, "cpu_pct_of_one_core": cpu }); }
            Err(e) => failures.push(format!("long prompt failed: {e}")) }
    }
    // 6. unload + reload
    let rss_before = rss(pid); eng.unload().await; let t = Instant::now();
    let mut eng2 = match Engine::load(net.clone(), cfg).await { Ok(e) => e, Err(e) => { eprintln!("reload failed: {e}"); return 1 } }; let reload_ms = t.elapsed().as_millis() as u64;
    let after = eng2.chat(&user("Say hello."), &GenParams { max_tokens: 16, ..GenParams::default() }, &AtomicBool::new(false), |_| {}, |_| {}).await; if after.is_err() || after.as_ref().map(|x| x.0.trim().is_empty()).unwrap_or(true) { failures.push("generation after reload failed".into()); }
    println!("reload: {reload_ms} ms (first load {load_ms} ms)"); eng2.unload().await;
    let s = net.ledger.summary();
    println!("\nSUMMARY: mean {} tok/s | mean first token {} ms | load {load_ms} ms | reload {reload_ms} ms | RAM idle {rss_idle:?} MB, before unload {rss_before:?} MB | external requests {} | local requests {}", mean(&tps).map(|x| format!("{x:.1}")).unwrap_or("?".into()), mean(&ttft).map(|x| format!("{x:.0}")).unwrap_or("?".into()), s.external_requests, s.local_requests);
    report["generations"] = json!(gens); report["mean_tok_per_s"] = json!(mean(&tps)); report["mean_first_token_ms"] = json!(mean(&ttft)); report["cancel_ms"] = json!(cancel_ms); report["reload_ms"] = json!(reload_ms); report["network"] = json!(s); report["failures"] = json!(failures);
    if let Some(o) = arg(&a, "--out") { let _ = std::fs::write(&o, serde_json::to_vec_pretty(&report).unwrap()); println!("report written to {o}"); }
    if failures.is_empty() { println!("CHECKS: all passed"); 0 } else { println!("CHECKS FAILED:"); for f in &failures { println!("  - {f}"); } if check { 1 } else { 0 } }
}
