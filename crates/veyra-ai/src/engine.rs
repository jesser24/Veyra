//! llama.cpp engine = supervised `llama-server` child process on loopback with a random port and a random API key (other local programs
//! cannot use it). One model at a time. Unload kills the process, which really frees the RAM. Cancel drops the HTTP connection, which makes
//! llama-server stop generating. All traffic goes through the central gateway as `LocalEngine` (never leaves the machine, enforced there).
use crate::context::Msg;
use serde::Serialize;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tokio::process::{Child, Command};
use veyra_net::{ErrorClass, Initiator, NetClient, NetError, Purpose};

#[derive(Clone, Debug)]
pub struct EngineConfig { pub server_bin: PathBuf, pub model: PathBuf, pub ctx: u32, pub threads: usize, pub load_timeout: Duration }

#[derive(Debug)]
pub enum EngineError { Spawn(String), ExitedEarly(String), LoadTimeout, Net(NetError), Protocol(String), Cancelled }
impl std::fmt::Display for EngineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self { EngineError::Spawn(e) => write!(f, "cannot start the inference engine: {e}"), EngineError::ExitedEarly(t) => write!(f, "the engine stopped while loading the model (likely not enough memory or a corrupt file): {t}"),
            EngineError::LoadTimeout => write!(f, "loading the model took too long"), EngineError::Net(e) => write!(f, "engine connection: {e:?}"), EngineError::Protocol(e) => write!(f, "unexpected engine reply: {e}"), EngineError::Cancelled => write!(f, "generation cancelled") }
    }
}
impl From<NetError> for EngineError { fn from(e: NetError) -> Self { EngineError::Net(e) } }

#[derive(Clone, Debug, Default, Serialize)]
pub struct Perf { pub prompt_tokens: Option<u64>, pub generated_tokens: Option<u64>, pub prompt_tokens_per_s: Option<f64>, pub generated_tokens_per_s: Option<f64>, pub wall_ms: u64, pub first_token_ms: Option<u64>, pub load_ms: Option<u64> }

#[derive(Clone, Debug, Serialize)]
pub struct Status { pub loaded: bool, pub model: Option<String>, pub ctx: u32, pub threads: usize, pub rss_mib: Option<u64>, pub load_ms: Option<u64> }

#[derive(Clone, Debug)]
pub struct GenParams { pub max_tokens: u32, pub temperature: f32, pub top_p: f32, pub thinking: bool }
impl Default for GenParams { fn default() -> Self { Self { max_tokens: 1024, temperature: 0.7, top_p: 0.8, thinking: false } } }

/// Longest silence tolerated mid-generation (long prompts on a slow CPU legitimately take minutes before the first token).
const STALL: Duration = Duration::from_secs(600);

pub struct Engine { child: Child, port: u16, key: String, net: NetClient, cfg: EngineConfig, load_ms: u64, log_tail: std::sync::Arc<std::sync::Mutex<String>> }

fn random_key() -> String { let mut b = [0u8; 16]; getrandom::fill(&mut b).expect("os rng"); b.iter().map(|x| format!("{x:02x}")).collect() }
fn free_port() -> std::io::Result<u16> { Ok(std::net::TcpListener::bind(("127.0.0.1", 0))?.local_addr()?.port()) }

impl Engine {
    /// Starts the engine. If the chosen port was taken between our probe and llama-server's bind (a race no probe can fully prevent),
    /// a fresh port is tried, up to 3 times. Any other failure is returned immediately.
    pub async fn load(net: NetClient, cfg: EngineConfig) -> Result<Engine, EngineError> {
        let mut last = None;
        for _ in 0..3 {
            match Engine::load_once(net.clone(), cfg.clone()).await {
                Err(EngineError::ExitedEarly(m)) if { let l = m.to_lowercase(); l.contains("address already in use") || l.contains("couldn't bind") || l.contains("could not bind") || l.contains("10048") } => last = Some(EngineError::ExitedEarly(m)),
                other => return other,
            }
        }
        Err(last.unwrap())
    }
    async fn load_once(net: NetClient, cfg: EngineConfig) -> Result<Engine, EngineError> {
        let port = free_port().map_err(|e| EngineError::Spawn(e.to_string()))?; let key = random_key(); let t0 = Instant::now();
        // The key goes through a private file, not the command line: command lines are readable by every local user in the process list.
        let keyfile = cfg.model.parent().unwrap_or(std::path::Path::new(".")).join(format!("engine-{port}.key"));
        write_private(&keyfile, &key).map_err(|e| EngineError::Spawn(format!("cannot write the engine key file: {e}")))?;
        let mut cmd = Command::new(&cfg.server_bin);
        cmd.arg("--model").arg(&cfg.model).args(["--host", "127.0.0.1", "--port"]).arg(port.to_string()).arg("--api-key-file").arg(&keyfile)
            .args(["--ctx-size", &cfg.ctx.to_string(), "--threads", &cfg.threads.to_string(), "--parallel", "1", "--jinja", "--no-webui"])
            .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
        #[cfg(windows)] { cmd.creation_flags(0x0800_0000); } // CREATE_NO_WINDOW: no console flash
        let spawned = cmd.spawn();
        let mut child = match spawned { Ok(c) => c, Err(e) => { let _ = std::fs::remove_file(&keyfile); return Err(EngineError::Spawn(format!("{}: {e}", cfg.server_bin.display()))); } };
        let log_tail = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        for s in [child.stdout.take().map(|s| Box::new(s) as Box<dyn tokio::io::AsyncRead + Unpin + Send>), child.stderr.take().map(|s| Box::new(s) as _)].into_iter().flatten() {
            let t = log_tail.clone();
            tokio::spawn(async move { use tokio::io::{AsyncBufReadExt, BufReader}; let mut l = BufReader::new(s).lines();
                while let Ok(Some(line)) = l.next_line().await { let mut g = t.lock().unwrap_or_else(|e| e.into_inner()); g.push_str(&line); g.push('\n'); if g.len() > 4000 { let cut = g.len() - 3000; let cut = (cut..g.len()).find(|i| g.is_char_boundary(*i)).unwrap_or(g.len()); *g = g[cut..].to_string(); } } });
        }
        let mut e = Engine { child, port, key, net, cfg, load_ms: 0, log_tail };
        let _guard = KeyFileGuard(keyfile); // removed as soon as loading ends (success or failure): the server has read it by then
        let deadline = Instant::now() + e.cfg.load_timeout;
        loop {
            if let Ok(Some(st)) = e.child.try_wait() { tokio::time::sleep(Duration::from_millis(100)).await; return Err(EngineError::ExitedEarly(format!("exit {st}; {}", e.tail()))); }
            if e.health().await { break; }
            if Instant::now() > deadline { let _ = e.child.kill().await; return Err(EngineError::LoadTimeout); }
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
        // Identity: /health needs no key, so something else listening on this port would also answer it. /props requires OUR random key,
        // and our own child must still be alive: together this proves the answering server is the one Veyra started.
        if !e.verify_identity().await { let _ = e.child.kill().await; return Err(EngineError::Protocol("the server on the engine port did not accept Veyra's key; refusing to use it".into())); }
        e.load_ms = t0.elapsed().as_millis() as u64;
        if let Some(pid) = e.child.id() { write_pidfile(&e.cfg, pid); }
        Ok(e)
    }
    pub fn port(&self) -> u16 { self.port }
    pub async fn verify_identity(&mut self) -> bool {
        if !self.is_alive() { return false; }
        match self.net.request(Purpose::LocalEngine, Initiator::Internal, reqwest::Method::GET, &self.url("/props"), &self.auth(), None, Some(Duration::from_secs(3))).await { Ok(mut r) => { let ok = r.status() == 200; while let Ok(Some(_)) = r.chunk().await {} ok } Err(_) => false }
    }
    fn tail(&self) -> String { self.log_tail.lock().unwrap_or_else(|e| e.into_inner()).trim().chars().rev().take(600).collect::<Vec<_>>().into_iter().rev().collect() }
    fn url(&self, p: &str) -> String { format!("http://127.0.0.1:{}{p}", self.port) }
    fn auth(&self) -> Vec<(&'static str, String)> { vec![("Authorization", format!("Bearer {}", self.key)), ("Content-Type", "application/json".into())] }
    async fn health(&self) -> bool {
        match self.net.request(Purpose::LocalEngine, Initiator::Internal, reqwest::Method::GET, &self.url("/health"), &[], None, Some(Duration::from_secs(2))).await { Ok(mut r) => { let ok = r.status() == 200; while let Ok(Some(_)) = r.chunk().await {} ok } Err(_) => false }
    }
    pub fn is_alive(&mut self) -> bool { matches!(self.child.try_wait(), Ok(None)) }
    pub fn status(&mut self) -> Status {
        let rss = self.child.id().and_then(rss_mib);
        Status { loaded: self.is_alive(), model: self.cfg.model.file_name().map(|n| n.to_string_lossy().into_owned()), ctx: self.cfg.ctx, threads: self.cfg.threads, rss_mib: rss, load_ms: Some(self.load_ms) }
    }
    /// Exact token count with the loaded model's tokenizer (used by the context builder).
    pub async fn count_tokens(&self, text: &str) -> Result<usize, EngineError> {
        let body = serde_json::to_vec(&serde_json::json!({ "content": text })).unwrap();
        let mut r = self.net.request(Purpose::LocalEngine, Initiator::Internal, reqwest::Method::POST, &self.url("/tokenize"), &self.auth(), Some(body), Some(Duration::from_secs(30))).await?;
        let mut buf = vec![]; while let Some(c) = r.chunk().await? { buf.extend_from_slice(&c); }
        let v: serde_json::Value = serde_json::from_slice(&buf).map_err(|e| EngineError::Protocol(e.to_string()))?;
        v.get("tokens").and_then(|t| t.as_array()).map(|a| a.len()).ok_or_else(|| EngineError::Protocol("no tokens".into()))
    }

    /// Streams the reply, calling `on_delta` with each piece of *answer* text (reasoning tokens are routed to `on_reasoning`). Returns the full
    /// answer and performance figures measured from the engine's own timings and the wall clock.
    pub async fn chat(&mut self, messages: &[Msg], p: &GenParams, cancel: &AtomicBool, mut on_delta: impl FnMut(&str), mut on_reasoning: impl FnMut(&str)) -> Result<(String, Perf), EngineError> {
        if !self.is_alive() { return Err(EngineError::Protocol(format!("engine is not running: {}", self.tail()))); }
        let body = serde_json::to_vec(&serde_json::json!({ "messages": messages, "stream": true, "max_tokens": p.max_tokens, "temperature": p.temperature, "top_p": p.top_p,
            "chat_template_kwargs": { "enable_thinking": p.thinking } })).unwrap();
        let t0 = Instant::now(); let mut last_data = Instant::now();
        let mut r = self.net.request(Purpose::LocalEngine, Initiator::Internal, reqwest::Method::POST, &self.url("/v1/chat/completions"), &self.auth(), Some(body), Some(Duration::from_secs(3600))).await?;
        if r.status() != 200 { let mut b = vec![]; while let Ok(Some(c)) = r.chunk().await { b.extend_from_slice(&c); if b.len() > 2000 { break; } } r.finish(Some(ErrorClass::Http)); return Err(EngineError::Protocol(format!("HTTP {}: {}", r.status(), String::from_utf8_lossy(&b)))); }
        let (mut answer, mut pending, mut perf) = (String::new(), String::new(), Perf { load_ms: Some(self.load_ms), ..Default::default() });
        'outer: loop {
            if cancel.load(Ordering::Relaxed) { r.finish(Some(ErrorClass::Cancelled)); return Err(EngineError::Cancelled); } // dropping `r` closes the socket: llama-server aborts the generation
            let chunk = match tokio::time::timeout(Duration::from_millis(250), r.chunk()).await { Err(_) => { if last_data.elapsed() > STALL { r.finish(Some(ErrorClass::Timeout)); return Err(EngineError::Protocol(format!("the engine produced nothing for {} s; generation abandoned", STALL.as_secs()))); } if !self.child.try_wait().map(|x| x.is_none()).unwrap_or(false) { return Err(EngineError::Protocol(format!("the engine process died during generation: {}", self.tail()))); } continue } Ok(Ok(Some(c))) => { last_data = Instant::now(); c } Ok(Ok(None)) => break, Ok(Err(e)) => return Err(e.into()) };
            pending.push_str(&String::from_utf8_lossy(&chunk));
            while let Some(i) = pending.find('\n') {
                let line = pending[..i].trim().to_string(); pending.drain(..=i);
                let Some(data) = line.strip_prefix("data:") else { continue }; let data = data.trim();
                if data == "[DONE]" { break 'outer; }
                let Ok(v) = serde_json::from_str::<serde_json::Value>(data) else { continue };
                if let Some(d) = v.pointer("/choices/0/delta") {
                    if let Some(t) = d.get("reasoning_content").and_then(|x| x.as_str()).filter(|s| !s.is_empty()) { on_reasoning(t); }
                    if let Some(t) = d.get("content").and_then(|x| x.as_str()).filter(|s| !s.is_empty()) { if perf.first_token_ms.is_none() { perf.first_token_ms = Some(t0.elapsed().as_millis() as u64); } answer.push_str(t); on_delta(t); }
                }
                if let Some(t) = v.get("timings") { perf.prompt_tokens = t.get("prompt_n").and_then(|x| x.as_u64()); perf.generated_tokens = t.get("predicted_n").and_then(|x| x.as_u64());
                    perf.prompt_tokens_per_s = t.get("prompt_per_second").and_then(|x| x.as_f64()); perf.generated_tokens_per_s = t.get("predicted_per_second").and_then(|x| x.as_f64()); }
            }
        }
        r.finish(None); perf.wall_ms = t0.elapsed().as_millis() as u64; Ok((answer, perf))
    }
    /// Frees the model's memory by ending the process.
    pub async fn unload(mut self) {
        let _ = self.child.start_kill();
        let _ = tokio::time::timeout(Duration::from_secs(10), self.child.wait()).await; // a process that survives 10 s is reported by reap_orphans at next start
        let _ = std::fs::remove_file(pidfile(&self.cfg));
    }
}

#[cfg(target_os = "linux")]
fn rss_mib(pid: u32) -> Option<u64> { std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?.lines().find(|l| l.starts_with("VmRSS:"))?.split_whitespace().nth(1)?.parse::<u64>().ok().map(|k| k / 1024) }
#[cfg(not(target_os = "linux"))]
fn rss_mib(pid: u32) -> Option<u64> { use sysinfo::{Pid, ProcessesToUpdate, System}; let mut s = System::new(); let p = Pid::from_u32(pid); s.refresh_processes(ProcessesToUpdate::Some(&[p]), true); s.process(p).map(|p| p.memory() / 1_048_576) }

fn pidfile(cfg: &EngineConfig) -> PathBuf { cfg.model.parent().unwrap_or(std::path::Path::new(".")).join("engine.pid") }
fn write_pidfile(cfg: &EngineConfig, pid: u32) { let _ = std::fs::write(pidfile(cfg), format!("{pid}\n{}\n", cfg.server_bin.display())); }

/// After a crash or forced quit the previous engine may still be running (it would hold gigabytes of RAM). At startup, if a pidfile exists
/// and that pid is STILL a process whose executable is our server binary, it is killed. Anything else (pid reused by an unrelated program) is left alone.
/// Returns true if an orphan was killed.
pub fn reap_orphans(dir: &std::path::Path, server_bin: &std::path::Path) -> bool {
    use sysinfo::{Pid, ProcessesToUpdate, System};
    let pf = dir.join("engine.pid"); let Ok(t) = std::fs::read_to_string(&pf) else { return false };
    let _ = std::fs::remove_file(&pf);
    let Some(pid) = t.lines().next().and_then(|l| l.trim().parse::<u32>().ok()) else { return false };
    let mut sys = System::new(); let p = Pid::from_u32(pid); sys.refresh_processes(ProcessesToUpdate::Some(&[p]), true);
    let Some(proc_) = sys.process(p) else { return false };
    let same = proc_.exe().map(|e| e == server_bin || e.file_name() == server_bin.file_name()).unwrap_or(false);
    if same { proc_.kill() } else { false }
}

struct KeyFileGuard(PathBuf);
impl Drop for KeyFileGuard { fn drop(&mut self) { let _ = std::fs::remove_file(&self.0); } }
fn write_private(p: &std::path::Path, content: &str) -> std::io::Result<()> {
    #[cfg(unix)] { use std::os::unix::fs::OpenOptionsExt; use std::io::Write; let mut f = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(p)?; f.write_all(content.as_bytes()) }
    #[cfg(not(unix))] { std::fs::write(p, content) } // Windows: the per-user app-data folder is already private to the user's account
}
