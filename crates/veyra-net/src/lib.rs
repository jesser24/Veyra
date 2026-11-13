//! Veyra's single network gateway. Every outbound connection of the application goes through `NetClient`; nothing else may open a socket
//! (enforced by review and by the self-test that watches the ledger). It provides:
//!  * a **policy**: purposes that must be started by the user (model download, web search, update check) are refused when `Initiator::Internal`;
//!  * **HTTPS only** for anything that is not loopback; TLS validation is never disabled; the system proxy settings are honoured;
//!  * a **ledger** of what the application itself did (host, purpose, time, bytes, latency, error class). It records host and method only:
//!    no URL path, no query, no headers, no body, no personal data. It cannot see traffic of the OS, the webview internals or other processes.
use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering::SeqCst};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Purpose { ModelDownload, WebSearch, Update, OtherService, LocalEngine }
impl Purpose {
    /// Must the user have explicitly triggered it? (Local engine traffic never leaves the machine.)
    pub fn needs_user_action(self) -> bool { !matches!(self, Purpose::LocalEngine | Purpose::OtherService) }
    pub fn essential(self) -> bool { matches!(self, Purpose::LocalEngine) }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Initiator { User, Internal }

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ErrorClass { Dns, Tls, Proxy, Timeout, Connect, Http, Cancelled, Io }

#[derive(Serialize, Clone, Debug)]
pub struct Entry {
    pub id: u64, pub purpose: Purpose, pub method: String, pub host: String, pub local: bool,
    pub started_ms: u64, pub duration_ms: Option<u64>, pub bytes_sent: u64, pub bytes_received: u64,
    pub status: Option<u16>, pub error: Option<ErrorClass>, pub in_flight: bool,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct Summary {
    pub requests: u64, pub in_flight: u64, pub errors: u64, pub dns_errors: u64, pub tls_errors: u64, pub proxy_errors: u64, pub timeouts: u64,
    pub bytes_sent: u64, pub bytes_received: u64, pub external_requests: u64, pub local_requests: u64,
    pub avg_latency_ms: Option<u64>, pub hosts: Vec<String>,
}

#[derive(Debug)]
pub enum NetError { Blocked(String), InsecureScheme, BadUrl(String), Failed(ErrorClass, String), Status(u16) }
impl std::fmt::Display for NetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NetError::Blocked(s) => write!(f, "blocked by network policy: {s}"),
            NetError::InsecureScheme => write!(f, "refusing a non-HTTPS connection to a non-local host"),
            NetError::BadUrl(s) => write!(f, "invalid URL: {s}"),
            NetError::Failed(c, s) => write!(f, "{c:?} error: {s}"),
            NetError::Status(c) => write!(f, "server answered HTTP {c}"),
        }
    }
}
impl std::error::Error for NetError {}

fn now_ms() -> u64 { SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0) }

/// In-memory, bounded, process-lifetime ledger. Never written to disk, never sent anywhere.
#[derive(Clone, Default)]
pub struct Ledger { inner: Arc<Mutex<Vec<Entry>>>, next: Arc<AtomicU64> }
const MAX_ENTRIES: usize = 2000;
impl Ledger {
    fn begin(&self, purpose: Purpose, method: &str, host: &str, local: bool) -> u64 {
        let id = self.next.fetch_add(1, SeqCst) + 1;
        let mut g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if g.len() >= MAX_ENTRIES { g.remove(0); }
        g.push(Entry { id, purpose, method: method.into(), host: host.into(), local, started_ms: now_ms(), duration_ms: None, bytes_sent: 0, bytes_received: 0, status: None, error: None, in_flight: true });
        id
    }
    fn update(&self, id: u64, f: impl FnOnce(&mut Entry)) { if let Some(e) = self.inner.lock().unwrap_or_else(|e| e.into_inner()).iter_mut().find(|e| e.id == id) { f(e); } }
    pub fn entries(&self) -> Vec<Entry> { self.inner.lock().unwrap_or_else(|e| e.into_inner()).clone() }
    pub fn clear(&self) { self.inner.lock().unwrap_or_else(|e| e.into_inner()).clear(); }
    pub fn summary(&self) -> Summary {
        let es = self.entries(); let mut s = Summary::default(); let (mut lat, mut n) = (0u64, 0u64);
        for e in &es {
            s.requests += 1; if e.in_flight { s.in_flight += 1; } if e.error.is_some() { s.errors += 1; }
            match e.error { Some(ErrorClass::Dns) => s.dns_errors += 1, Some(ErrorClass::Tls) => s.tls_errors += 1, Some(ErrorClass::Proxy) => s.proxy_errors += 1, Some(ErrorClass::Timeout) => s.timeouts += 1, _ => {} }
            s.bytes_sent += e.bytes_sent; s.bytes_received += e.bytes_received;
            if e.local { s.local_requests += 1 } else { s.external_requests += 1; if !s.hosts.contains(&e.host) { s.hosts.push(e.host.clone()); } }
            if let Some(d) = e.duration_ms { lat += d; n += 1; }
        }
        if n > 0 { s.avg_latency_ms = Some(lat / n); } s
    }
}

pub fn classify(e: &reqwest::Error) -> ErrorClass {
    if e.is_timeout() { return ErrorClass::Timeout; }
    let mut chain = format!("{e}"); let mut src: Option<&dyn std::error::Error> = std::error::Error::source(e);
    while let Some(s) = src { chain.push_str(" | "); chain.push_str(&s.to_string()); src = s.source(); }
    let c = chain.to_lowercase();
    if c.contains("proxy") || c.contains("tunnel") { ErrorClass::Proxy }
    else if c.contains("dns") || c.contains("lookup address") || c.contains("name or service not known") || c.contains("no such host") || c.contains("nodename nor servname") { ErrorClass::Dns }
    else if c.contains("certificate") || c.contains("tls") || c.contains("handshake") || c.contains("ssl") { ErrorClass::Tls }
    else if e.is_connect() { ErrorClass::Connect } else { ErrorClass::Io }
}

pub struct Config { pub connect_timeout: Duration, pub request_timeout: Duration, pub user_agent: String }
impl Default for Config { fn default() -> Self { Config { connect_timeout: Duration::from_secs(15), request_timeout: Duration::from_secs(60), user_agent: format!("Veyra/{}", env!("CARGO_PKG_VERSION")) } } }

#[derive(Clone)]
pub struct NetClient { external: reqwest::Client, local: reqwest::Client, pub ledger: Ledger, relax_loopback: bool }

fn is_loopback(u: &url::Url) -> bool { match u.host() { Some(url::Host::Domain(d)) => d == "localhost", Some(url::Host::Ipv4(a)) => a.is_loopback(), Some(url::Host::Ipv6(a)) => a.is_loopback(), None => false } }

impl NetClient {
    pub fn new(cfg: Config) -> Result<NetClient, NetError> {
        // External: system proxy settings are honoured by default; certificates are validated against the OS trust store.
        let external = reqwest::Client::builder().connect_timeout(cfg.connect_timeout).user_agent(cfg.user_agent.clone()).https_only(true).build().map_err(|e| NetError::Failed(ErrorClass::Io, e.to_string()))?;
        // Local (loopback only): never uses a proxy, so engine traffic cannot leave the machine through one.
        let local = reqwest::Client::builder().no_proxy().connect_timeout(Duration::from_secs(3)).build().map_err(|e| NetError::Failed(ErrorClass::Io, e.to_string()))?;
        Ok(NetClient { external, local, ledger: Ledger::default(), relax_loopback: false })
    }

    /// TEST SUPPORT ONLY (cargo feature `test-support`, never enabled in the app): lets any purpose talk plain HTTP to loopback so downloads can be
    /// tested against a local server. The app binary does not enable the feature, so the production policy below is the only one that ships.
    #[cfg(feature = "test-support")]
    pub fn new_for_tests(cfg: Config) -> Result<NetClient, NetError> { let mut c = NetClient::new(cfg)?; c.relax_loopback = true; Ok(c) }

    /// Start a request. `timeout` applies to the whole exchange for non-streaming calls; streaming callers pass `None` and cancel themselves.
    pub async fn request(&self, purpose: Purpose, who: Initiator, method: reqwest::Method, url: &str, headers: &[(&str, String)], body: Option<Vec<u8>>, timeout: Option<Duration>) -> Result<Tracked, NetError> {
        let u = url::Url::parse(url).map_err(|e| NetError::BadUrl(e.to_string()))?;
        let local = is_loopback(&u);
        if purpose.needs_user_action() && who != Initiator::User { return Err(NetError::Blocked(format!("{purpose:?} may only start from a user action"))); }
        if !(self.relax_loopback && local) && local != (purpose == Purpose::LocalEngine) { return Err(NetError::Blocked(if local { "loopback addresses are reserved for the local engine".into() } else { "the local engine may only talk to loopback".into() })); }
        if !local && u.scheme() != "https" { return Err(NetError::InsecureScheme); }
        let host = u.host_str().unwrap_or("").to_string();
        let id = self.ledger.begin(purpose, method.as_str(), &host, local);
        let started = Instant::now();
        let mut rb = (if local { &self.local } else { &self.external }).request(method, u);
        for (k, v) in headers { rb = rb.header(*k, v); }
        if let Some(b) = body { self.ledger.update(id, |e| e.bytes_sent = b.len() as u64); rb = rb.body(b); }
        if let Some(t) = timeout { rb = rb.timeout(t); }
        match rb.send().await {
            Ok(resp) => { let st = resp.status().as_u16(); self.ledger.update(id, |e| e.status = Some(st)); Ok(Tracked { resp, ledger: self.ledger.clone(), id, started, done: false }) }
            Err(e) => { let c = classify(&e); self.ledger.update(id, |x| { x.in_flight = false; x.error = Some(c); x.duration_ms = Some(started.elapsed().as_millis() as u64); }); Err(NetError::Failed(c, e.without_url().to_string())) }
        }
    }

    /// Small GET returning the body, capped at `limit` bytes.
    pub async fn get_bytes(&self, purpose: Purpose, who: Initiator, url: &str, limit: usize) -> Result<Vec<u8>, NetError> {
        let mut t = self.request(purpose, who, reqwest::Method::GET, url, &[], None, Some(Duration::from_secs(60))).await?;
        if !(200..300).contains(&t.status()) { let s = t.status(); t.finish(Some(ErrorClass::Http)); return Err(NetError::Status(s)); }
        let mut out = vec![];
        while let Some(c) = t.chunk().await? { out.extend_from_slice(&c); if out.len() > limit { t.finish(Some(ErrorClass::Io)); return Err(NetError::Failed(ErrorClass::Io, "response larger than the allowed limit".into())); } }
        t.finish(None); Ok(out)
    }
}

/// A response whose bytes are counted in the ledger as they are consumed. Call `finish` (or drop: counted as cancelled).
pub struct Tracked { resp: reqwest::Response, ledger: Ledger, id: u64, started: Instant, done: bool }
impl Tracked {
    pub fn status(&self) -> u16 { self.resp.status().as_u16() }
    pub fn header(&self, name: &str) -> Option<String> { self.resp.headers().get(name).and_then(|v| v.to_str().ok()).map(String::from) }
    pub async fn chunk(&mut self) -> Result<Option<bytes::Bytes>, NetError> {
        match self.resp.chunk().await {
            Ok(Some(b)) => { let n = b.len() as u64; self.ledger.update(self.id, |e| e.bytes_received += n); Ok(Some(b)) }
            Ok(None) => Ok(None),
            Err(e) => { let c = classify(&e); self.finish(Some(c)); Err(NetError::Failed(c, e.without_url().to_string())) }
        }
    }
    pub fn finish(&mut self, err: Option<ErrorClass>) {
        if self.done { return; } self.done = true;
        let ms = self.started.elapsed().as_millis() as u64;
        self.ledger.update(self.id, |e| { e.in_flight = false; e.error = err; e.duration_ms = Some(ms); });
    }
}
impl Drop for Tracked { fn drop(&mut self) { self.finish(Some(ErrorClass::Cancelled)); } }
