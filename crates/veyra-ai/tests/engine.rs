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

#[tokio::test] async fn engine_requires_its_random_key_and_listens_on_loopback_only() {
    let d = tempfile::tempdir().unwrap(); let n = net(); let e = Engine::load(n.clone(), cfg(model(d.path(), b"x"))).await.unwrap();
    // direct call WITHOUT the key must be refused (401); /health is the only open endpoint, as in llama-server
    let url = format!("http://127.0.0.1:{}/v1/chat/completions", e.port());
    let r = n.request(veyra_net::Purpose::LocalEngine, veyra_net::Initiator::Internal, reqwest::Method::POST, &url, &[], Some(b"{}".to_vec()), None).await.unwrap(); assert_eq!(r.status(), 401);
    // bound to 127.0.0.1 and not to the LAN address: connecting through a non-loopback local IP must fail
    if let Some(ip) = lan_ip() { assert!(std::net::TcpStream::connect_timeout(&(ip, e.port()).into(), Duration::from_millis(500)).is_err(), "engine reachable on {ip}"); }
    e.unload().await;
}
fn lan_ip() -> Option<std::net::IpAddr> { let s = std::net::UdpSocket::bind("0.0.0.0:0").ok()?; s.connect("192.0.2.1:9").ok()?; let ip = s.local_addr().ok()?.ip(); (!ip.is_loopback() && !ip.is_unspecified()).then_some(ip) }

#[tokio::test] async fn unload_really_ends_the_process_and_orphans_from_a_crash_are_reaped() {
    let d = tempfile::tempdir().unwrap(); let c = cfg(model(d.path(), b"x")); let mut e = Engine::load(net(), c.clone()).await.unwrap();
    let pid = std::fs::read_to_string(d.path().join("engine.pid")).unwrap().lines().next().unwrap().parse::<u32>().unwrap();
    assert!(e.is_alive()); drop(e.status()); e.unload().await; assert!(!d.path().join("engine.pid").exists(), "pidfile removed on clean unload"); let _ = pid;
    // simulate a crash of Veyra: engine started by a "previous run" keeps running with its pidfile left behind
    let child = std::process::Command::new(&c.server_bin).args(["--model", c.model.to_str().unwrap(), "--port", "0", "--api-key", "k"]).spawn().unwrap();
    std::fs::write(d.path().join("engine.pid"), format!("{}\n", child.id())).unwrap();
    assert!(reap_orphans(d.path(), &c.server_bin), "orphan must be killed"); let mut child = child; std::thread::sleep(Duration::from_millis(300)); assert!(child.try_wait().unwrap().is_some(), "orphan is gone");
    // an unrelated process whose pid is in a stale pidfile must NOT be killed
    let mut other = std::process::Command::new(if cfg!(windows) { "cmd" } else { "sleep" }).args(if cfg!(windows) { vec!["/c", "ping -n 5 127.0.0.1 >nul"] } else { vec!["5"] }).spawn().unwrap();
    std::fs::write(d.path().join("engine.pid"), format!("{}\n", other.id())).unwrap();
    assert!(!reap_orphans(d.path(), &c.server_bin)); assert!(other.try_wait().unwrap().is_none(), "unrelated process untouched"); let _ = other.kill();
}
