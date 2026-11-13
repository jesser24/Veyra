use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use veyra_ai::context::Msg;
use veyra_ai::engine::*;
use veyra_net::{Config, NetClient};

fn net() -> NetClient { NetClient::new(Config { connect_timeout: Duration::from_secs(3), request_timeout: Duration::from_secs(30), user_agent: "veyra-test".into() }).unwrap() }
fn model(dir: &std::path::Path, bytes: &[u8]) -> PathBuf { let p = dir.join("m.gguf"); std::fs::write(&p, bytes).unwrap(); p }
fn cfg(model: PathBuf) -> EngineConfig { EngineConfig { server_bin: PathBuf::from(env!("CARGO_BIN_EXE_fake-llama-server")), model, ctx: 4096, threads: 2, load_timeout: Duration::from_secs(10) } }
fn user(s: &str) -> Vec<Msg> { vec![Msg { role: "user".into(), content: s.into() }] }

#[tokio::test] async fn loads_streams_reports_perf_and_unloads() {
    let d = tempfile::tempdir().unwrap(); let n = net(); let mut e = Engine::load(n.clone(), cfg(model(d.path(), b"x"))).await.unwrap();
    assert!(e.status().loaded);
    let mut pieces = vec![]; let (text, perf) = e.chat(&user("hi"), &GenParams::default(), &AtomicBool::new(false), |t| pieces.push(t.to_string()), |_| {}).await.unwrap();
    assert!(pieces.len() > 3, "must arrive as a stream, not one block"); assert_eq!(text, "Hello from the fake engine. Done!");
    assert_eq!(perf.generated_tokens, Some(8)); assert_eq!(perf.generated_tokens_per_s, Some(12.5)); assert!(perf.first_token_ms.is_some());
    assert_eq!(e.count_tokens("a b c").await.unwrap(), 3);
    let s = n.ledger.summary(); assert!(s.local_requests >= 3 && s.external_requests == 0, "engine traffic is loopback only and recorded as such");
    e.unload().await;
}
#[tokio::test] async fn cancel_stops_generation_quickly_and_engine_stays_usable() {
    let d = tempfile::tempdir().unwrap(); let mut e = Engine::load(net(), cfg(model(d.path(), b"x"))).await.unwrap();
    let cancel = AtomicBool::new(false); let mut n = 0;
    let r = e.chat(&user("long"), &GenParams::default(), &cancel, |_| { n += 1; if n == 5 { cancel.store(true, Ordering::Relaxed); } }, |_| {}).await;
    assert!(matches!(r, Err(EngineError::Cancelled))); assert!(n < 20, "stopped after {n} tokens");
    let (t, _) = e.chat(&user("again"), &GenParams::default(), &AtomicBool::new(false), |_| {}, |_| {}).await.unwrap(); assert!(t.starts_with("Hello"));
    e.unload().await;
}
#[tokio::test] async fn a_model_that_cannot_load_gives_a_clear_error_with_the_engine_log() {
    let d = tempfile::tempdir().unwrap(); let r = Engine::load(net(), cfg(model(d.path(), b""))).await;
    match r { Err(EngineError::ExitedEarly(m)) => assert!(m.contains("failed to load model"), "{m}"), Err(e) => panic!("{e}"), Ok(_) => panic!("must fail") }
}
#[tokio::test] async fn missing_binary_and_slow_loads_are_reported() {
    let d = tempfile::tempdir().unwrap(); let m = model(d.path(), b"x");
    let mut c = cfg(m.clone()); c.server_bin = d.path().join("nope"); assert!(matches!(Engine::load(net(), c).await, Err(EngineError::Spawn(_))));
    std::env::set_var("FAKE_LOAD_MS", "3000"); let mut c = cfg(m); c.load_timeout = Duration::from_millis(600);
    assert!(matches!(Engine::load(net(), c).await, Err(EngineError::LoadTimeout))); std::env::remove_var("FAKE_LOAD_MS");
}
