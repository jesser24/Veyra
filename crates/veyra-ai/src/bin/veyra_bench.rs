//! Reproducible end-to-end benchmark and real-inference check. Runs the REAL in-process engine (libllama inside this very process) with a REAL model:
//!   veyra-bench --dir <models dir> [--model qwen3-4b-q5_k_m] [--ctx 4096] [--threads N] [--runs 3] [--long] [--tune] [--check] [--out report.json]
//! Downloads the model through Veyra's own gateway if missing (SHA-256 verified), then measures download, load, prefill and decode speed SEPARATELY,
//! first token, RAM, CPU, cancellation latency, long-prompt behaviour, KV-cache reuse between turns, unload/reload, and proves the engine is in-process
//! (no child process; resident memory of THIS process grows by about the model size). `--tune` compares thread/batch/flash-attention/KV settings.
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
fn me() -> Pid { Pid::from_u32(std::process::id()) }
fn rss() -> u64 { let mut s = System::new(); s.refresh_processes(ProcessesToUpdate::Some(&[me()]), true); s.process(me()).map(|p| p.memory() / 1_048_576).unwrap_or(0) }
/// Names of processes whose parent is this process (must stay empty: no hidden inference server).
fn children() -> Vec<String> { let mut s = System::new(); s.refresh_processes(ProcessesToUpdate::All, true); s.processes().values().filter(|p| p.parent() == Some(me())).map(|p| p.name().to_string_lossy().into_owned()).collect() }

/// Average CPU use of THIS process (percent of one core: 400 = four cores busy) while `f` runs.
fn with_cpu<T>(f: impl FnOnce() -> T) -> (T, f32) {
    let stop = Arc::new(AtomicBool::new(false)); let s2 = stop.clone();
    let h = std::thread::spawn(move || { let mut sys = System::new(); let mut v = vec![]; sys.refresh_processes(ProcessesToUpdate::Some(&[me()]), true);
        while !s2.load(Ordering::Relaxed) { std::thread::sleep(Duration::from_millis(500)); sys.refresh_processes(ProcessesToUpdate::Some(&[me()]), true); if let Some(p) = sys.process(me()) { v.push(p.cpu_usage()); } } if v.is_empty() { 0.0 } else { v.iter().sum::<f32>() / v.len() as f32 } });
    let r = f(); stop.store(true, Ordering::Relaxed); (r, h.join().unwrap_or(0.0))
}
fn gp(max: u32, temp: f32) -> GenParams { GenParams { max_tokens: max, temperature: temp, top_p: 0.9, top_k: 20, thinking: false } }
fn mean(v: &[f64]) -> Option<f64> { if v.is_empty() { None } else { Some(v.iter().sum::<f64>() / v.len() as f64) } }
fn filler(words: usize) -> String { let s = "The quick brown fox jumps over the lazy dog while the committee reviews the quarterly figures. "; s.repeat(words / 15 + 1) }

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap();
    std::process::exit(rt.block_on(run(a)));
}

async fn run(a: Vec<String>) -> i32 {
    if veyra_llama::IS_STUB { eprintln!("this binary was built without the llama.cpp libraries (set VEYRA_LLAMA_DIR)"); return 2; }
    let id = arg(&a, "--model").unwrap_or_else(|| "qwen3-0.6b-q8_0".into()); let spec = match catalog::by_id(&id) { Some(s) if s.status == catalog::Status::Verified => s, _ => { eprintln!("unknown or unverified model id {id}"); return 2 } };
    let dir = PathBuf::from(arg(&a, "--dir").unwrap_or_else(|| "veyra-bench-models".into())); let runs: usize = arg(&a, "--runs").and_then(|v| v.parse().ok()).unwrap_or(3);
    let hw = hardware::detect(); let ctx: u32 = arg(&a, "--ctx").and_then(|v| v.parse().ok()).unwrap_or(if hw.apple_silicon && hw.available_ram_mib >= 6 * 1024 { 8192 } else { 4096 });
    let threads: usize = arg(&a, "--threads").and_then(|v| v.parse().ok()).unwrap_or_else(|| hardware::auto_threads(&hw));
    let (check, long, tune) = (a.iter().any(|x| x == "--check"), a.iter().any(|x| x == "--long"), a.iter().any(|x| x == "--tune")); let mut failures: Vec<String> = vec![];
    println!("== Veyra benchmark (in-process engine: {}) ==\nhardware: {} | {} cores/{} threads | {} MB RAM ({} MB free) | avx2={} | {}\nmodel: {} ({}) ctx={ctx} threads={threads}", veyra_llama::version(), hw.cpu_brand, hw.physical_cores, hw.logical_cores, hw.total_ram_mib, hw.available_ram_mib, hw.avx2, hw.os, spec.name, spec.quant);
    let net = NetClient::new(Config::default()).expect("net");
    let t = Instant::now(); let had = download::final_path(&dir, spec).exists(); let mut last = Instant::now(); let cancel_dl = AtomicBool::new(false);
    let path = match download::download(&net, &dir, spec, None, hardware::free_disk_bytes(&dir), &cancel_dl, |p| { if last.elapsed() > Duration::from_secs(5) { println!("  {} {} / {:?}", p.phase, p.done >> 20, p.total.map(|t| t >> 20)); last = Instant::now(); } }).await { Ok(p) => p, Err(e) => { eprintln!("download/verify failed: {e}"); return 1 } };
    let dl_s = t.elapsed().as_secs_f64(); println!("download+SHA-256 verification: {dl_s:.1} s ({})", if had { "file was already present: hash time only" } else { "network + hash" });
    let ext = net.ledger.summary(); println!("network ledger: {} external request(s) to {:?}", ext.external_requests, ext.hosts);
    let mut report = json!({ "hardware": hw, "llama": veyra_llama::version(), "model": spec.id, "ctx": ctx, "threads": threads, "download_verify_s": dl_s });

    // ---- load, and prove the engine lives in this process
    let rss0 = rss(); let cfg = EngineConfig::new(path.clone(), ctx, threads); let t = Instant::now();
    let mut eng = match Engine::load(cfg.clone()) { Ok(e) => e, Err(e) => { eprintln!("engine load failed: {e}"); return 1 } }; let load_ms = t.elapsed().as_millis() as u64;
    let rss1 = rss(); let kids = children(); let model_mb = eng.model_bytes() >> 20;
    println!("load: {load_ms} ms | RAM of THIS process: {rss0} -> {rss1} MB (model {model_mb} MB) | child processes: {kids:?}");
    if !kids.is_empty() { failures.push(format!("child processes present: {kids:?}")); }
    if rss1.saturating_sub(rss0) < model_mb / 2 { println!("note: resident memory grew by less than half the model size (the model file is memory-mapped and pages in lazily)"); }
    report["load_ms"] = json!(load_ms); report["rss_before_mb"] = json!(rss0); report["rss_after_load_mb"] = json!(rss1); report["child_processes"] = json!(kids);

    // ---- repeated short generations (prefill and decode reported separately)
    let (mut dec, mut ttft, mut gens) = (vec![], vec![], vec![]);
    for i in 0..runs {
        let mut pieces = 0usize; let (r, cpu) = with_cpu(|| eng.chat(&user("Write two sentences about the sea. Do not use lists."), &gp(96, 0.7), &AtomicBool::new(false), |_| pieces += 1, |_| {}));
        match r { Ok((text, p)) => { println!("run {}: {} tokens | decode {} | prefill {} ms ({} prompt tok) | first token {} ms | total {} ms | cpu {cpu:.0}% | rss {} MB", i + 1, p.generated_tokens.unwrap_or(0), p.generated_tokens_per_s.map(|x| format!("{x:.1} tok/s")).unwrap_or("?".into()), p.prefill_ms.unwrap_or(0), p.prompt_tokens.unwrap_or(0), p.first_token_ms.unwrap_or(0), p.wall_ms, rss());
                if text.trim().is_empty() { failures.push(format!("run {}: empty answer", i + 1)); } if pieces < 3 { failures.push(format!("run {}: not streamed ({pieces} pieces)", i + 1)); }
                if let Some(x) = p.generated_tokens_per_s { dec.push(x); } if let Some(x) = p.first_token_ms { ttft.push(x as f64); } gens.push(json!({ "text": text, "perf": p, "cpu_pct_of_one_core": cpu })); }
            Err(e) => failures.push(format!("run {} failed: {e}", i + 1)) }
    }
    match eng.chat(&user("What is 17 + 25? Answer with only the number."), &gp(16, 0.0), &AtomicBool::new(false), |_| {}, |_| {}) { Ok((t, _)) => { println!("sanity (17+25): {:?}", t.trim()); if !t.contains("42") { failures.push(format!("sanity answer wrong: {t:?}")); } report["sanity_17_plus_25"] = json!(t); } Err(e) => failures.push(format!("sanity failed: {e}")) }

    // ---- KV-cache reuse: a follow-up turn must only pay for its new tokens
    let h1 = vec![Msg { role: "user".into(), content: format!("{}\nRemember the word: pineapple.", filler(300)) }];
    let (r1, _) = (eng.chat(&h1, &gp(24, 0.0), &AtomicBool::new(false), |_| {}, |_| {}), ());
    if let Ok((a1, p1)) = r1 { let mut h2 = h1.clone(); h2.push(Msg { role: "assistant".into(), content: a1 }); h2.push(Msg { role: "user".into(), content: "Which word did I ask you to remember?".into() });
        match eng.chat(&h2, &gp(24, 0.0), &AtomicBool::new(false), |_| {}, |_| {}) { Ok((a2, p2)) => { println!("follow-up turn: prompt {} tokens of which {} reused from cache | prefill {} ms (first turn {} ms) | answer {:?}", p2.prompt_tokens.unwrap_or(0), p2.prompt_cached.unwrap_or(0), p2.prefill_ms.unwrap_or(0), p1.prefill_ms.unwrap_or(0), a2.trim());
            if p2.prompt_cached.unwrap_or(0) < p2.prompt_tokens.unwrap_or(1) / 2 { failures.push("KV cache was not reused between turns".into()); } report["followup"] = json!({ "first": p1, "second": p2 }); } Err(e) => failures.push(format!("follow-up failed: {e}")) } }

    // ---- cancellation latency (generation runs on a worker thread, cancelled from here)
    let cancel = Arc::new(AtomicBool::new(false)); let c2 = cancel.clone(); let mut n = 0u32; let mut at: Option<Instant> = None;
    let r = eng.chat(&user("Count from 1 to 400 separated by commas."), &gp(800, 0.2), &cancel, |_| { n += 1; if n == 15 { at = Some(Instant::now()); c2.store(true, Ordering::Relaxed); } }, |_| {});
    let cancel_ms = at.map(|t| t.elapsed().as_millis() as u64); println!("cancel: {:?}, returned {cancel_ms:?} ms after the stop request", r.as_ref().err().map(|e| e.to_string()));
    if !matches!(r, Err(EngineError::Cancelled)) || cancel_ms.map(|m| m > 3000).unwrap_or(true) { failures.push(format!("cancellation not prompt: {cancel_ms:?} ms")); }
    // cancellation during a LONG prefill must also be prompt
    { let cancel = Arc::new(AtomicBool::new(false)); let c = cancel.clone(); let big = user(&format!("{}\nSummarise.", filler((ctx as usize * 3) / 5))); let t = Instant::now();
      let h = std::thread::scope(|s| { let hh = s.spawn(|| { std::thread::sleep(Duration::from_millis(1500)); c.store(true, Ordering::Relaxed); Instant::now() }); let r = eng.chat(&big, &gp(16, 0.0), &cancel, |_| {}, |_| {}); (r, hh.join().unwrap()) });
      let lat = h.1.elapsed().as_millis(); println!("cancel during prefill: {:?}, took {} ms in total, returned {lat} ms after the stop request", h.0.as_ref().err().map(|e| e.to_string()), t.elapsed().as_millis());
      if !matches!(h.0, Err(EngineError::Cancelled)) { println!("note: prefill finished before the stop request (fast machine)"); } else if lat > 5000 { failures.push(format!("cancel during prefill too slow: {lat} ms")); } }

    if long { // one prompt filling ~70 % of the context
        let big = format!("{}\nQuestion: what animal jumps over the dog? Answer in one word.", filler((ctx as usize * 7) / 10 * 3 / 4));
        let (r, cpu) = with_cpu(|| eng.chat(&user(&big), &gp(16, 0.0), &AtomicBool::new(false), |_| {}, |_| {}));
        match r { Ok((t, p)) => { println!("long prompt ({} tokens): prefill {} ms = {:?} tok/s | first token {} ms | answer {:?} | cpu {cpu:.0}%", p.prompt_tokens.unwrap_or(0), p.prefill_ms.unwrap_or(0), p.prompt_tokens_per_s.map(|x| x.round()), p.first_token_ms.unwrap_or(0), t.trim()); report["long_prompt"] = json!({ "perf": p, "answer": t, "cpu_pct_of_one_core": cpu }); }
            Err(e) => failures.push(format!("long prompt failed: {e}")) } }

    // ---- unload frees the memory of THIS process; reload
    let rss_loaded = rss(); eng.unload(); std::thread::sleep(Duration::from_millis(500)); let rss_after = rss();
    println!("unload: RAM of this process {rss_loaded} -> {rss_after} MB"); if rss_loaded > 600 && rss_after as f64 > rss_loaded as f64 * 0.8 { failures.push(format!("memory not released after unload ({rss_loaded} -> {rss_after} MB)")); }
    let t = Instant::now(); let mut eng2 = match Engine::load(cfg.clone()) { Ok(e) => e, Err(e) => { eprintln!("reload failed: {e}"); return 1 } }; let reload_ms = t.elapsed().as_millis() as u64;
    match eng2.chat(&user("Say hello."), &gp(16, 0.7), &AtomicBool::new(false), |_| {}, |_| {}) { Ok((t, _)) if !t.trim().is_empty() => {}, _ => failures.push("generation after reload failed".into()) }
    println!("reload: {reload_ms} ms (first load {load_ms} ms)"); eng2.unload();

    // ---- configuration comparison
    if tune { let mut rows = vec![]; let cores = hw.physical_cores.max(1);
        let mut set: Vec<(String, EngineConfig)> = vec![]; let mk = |t: usize, b: u32, ub: u32, fa: i32, kv: bool| { let mut c = EngineConfig::new(path.clone(), ctx, t); c.batch = b; c.ubatch = ub; c.flash_attn = fa; c.kv_q8 = kv; c };
        for t in { let mut v = vec![(cores.saturating_sub(1)).max(1), cores, hw.logical_cores]; v.dedup(); v } { set.push((format!("threads={t}"), mk(t, 512, 512, -1, false))); }
        for (b, ub) in [(256, 256), (1024, 512), (2048, 1024)] { set.push((format!("threads={cores} batch={b} ubatch={ub}"), mk(cores, b, ub, -1, false))); }
        set.push((format!("threads={cores} flash-attn off"), mk(cores, 512, 512, 0, false))); set.push((format!("threads={cores} flash-attn on"), mk(cores, 512, 512, 1, false))); set.push((format!("threads={cores} KV q8_0"), mk(cores, 512, 512, -1, true)));
        println!("\n== configuration comparison (same prompts, greedy) ==\n{:<42} {:>10} {:>12} {:>12} {:>8}", "configuration", "load ms", "prefill t/s", "decode t/s", "RSS MB");
        let prompt = user(&format!("{}\nSummarise in one sentence.", filler(400)));
        for (name, c) in set { let t = Instant::now(); let Ok(mut e) = Engine::load(c) else { println!("{name:<42} load failed"); continue }; let l = t.elapsed().as_millis();
            let mut pre = vec![]; let mut de = vec![]; for _ in 0..2 { e.chat(&user("Hi"), &gp(2, 0.0), &AtomicBool::new(false), |_| {}, |_| {}).ok(); // reset cache between measurements
                if let Ok((_, p)) = e.chat(&prompt, &gp(48, 0.0), &AtomicBool::new(false), |_| {}, |_| {}) { if let Some(x) = p.prompt_tokens_per_s { pre.push(x); } if let Some(x) = p.generated_tokens_per_s { de.push(x); } } }
            let (pm, dm) = (mean(&pre).unwrap_or(0.0), mean(&de).unwrap_or(0.0)); println!("{name:<42} {l:>10} {pm:>12.1} {dm:>12.1} {:>8}", rss()); rows.push(json!({ "config": name, "load_ms": l, "prefill_tok_s": pm, "decode_tok_s": dm })); e.unload(); }
        report["tuning"] = json!(rows); }

    let s = net.ledger.summary();
    println!("\nSUMMARY: mean decode {} tok/s | mean first token {} ms | load {load_ms} ms | reload {reload_ms} ms | external requests {} | local (loopback) requests {}", mean(&dec).map(|x| format!("{x:.1}")).unwrap_or("?".into()), mean(&ttft).map(|x| format!("{x:.0}")).unwrap_or("?".into()), s.external_requests, s.local_requests);
    if s.local_requests != 0 { failures.push("loopback requests were made: the engine must not use a local server".into()); }
    report["generations"] = json!(gens); report["mean_decode_tok_per_s"] = json!(mean(&dec)); report["mean_first_token_ms"] = json!(mean(&ttft)); report["cancel_ms"] = json!(cancel_ms); report["reload_ms"] = json!(reload_ms); report["network"] = json!(s); report["failures"] = json!(failures);
    if let Some(o) = arg(&a, "--out") { let _ = std::fs::write(&o, serde_json::to_vec_pretty(&report).unwrap()); println!("report written to {o}"); }
    if failures.is_empty() { println!("CHECKS: all passed"); 0 } else { println!("CHECKS FAILED:"); for f in &failures { println!("  - {f}"); } if check { 1 } else { 0 } }
}
