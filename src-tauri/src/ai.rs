//! Tauri layer over `veyra-ai` / `veyra-net` / `veyra-store`. Every command is `async` (never blocks the UI thread, and window-creating
//! commands must be async on Windows); long work runs in spawned tasks that report through events:
//!   `ai-download`  {id, phase, done, total, error}      `ai-engine`  {…status…}
//!   `chat-token`   {conversation, message, delta}        `chat-done`  {conversation, message, perf, cancelled}   `chat-error` {conversation, message, error}
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as Std};
use std::time::{Duration, Instant};
use tauri::async_runtime::{spawn, spawn_blocking, Mutex};
use tauri::{AppHandle, Emitter, Manager, State};
use veyra_ai::catalog::{self, ModelSpec};
use veyra_ai::context::{self, Msg};
use veyra_ai::download;
use veyra_ai::engine::{Engine, EngineConfig, EngineError, GenParams, Perf};
use veyra_ai::hardware;
use veyra_ai::installed::{self, FileState};
use veyra_net::{Config, NetClient};
use veyra_store::crypto::KdfParams;
use veyra_store::db::{Conversation, Message, Store, View};
use veyra_store::vault;

pub struct Ai {
    pub net: NetClient, pub data: PathBuf, models: PathBuf,
    engine: Mutex<Option<Engine>>, loaded: Std<Option<String>>, last_status: Std<Option<veyra_ai::engine::Status>>,
    dl_cancel: Std<Option<Arc<AtomicBool>>>, gen_cancel: Std<Option<Arc<AtomicBool>>>, busy_dl: AtomicBool,
    store: Std<Option<Store>>, last_perf: Std<Option<Perf>>, tok_cache: Std<HashMap<u64, usize>>,
}
type A<'a> = State<'a, Arc<Ai>>;

impl Ai {
    pub fn new(data: PathBuf) -> Result<Ai, String> {
        let net = NetClient::new(Config::default()).map_err(|e| format!("network gateway: {e:?}"))?;
        let models = data.join("models"); let _ = std::fs::create_dir_all(&models);
        Ok(Ai { net, data, models, engine: Mutex::new(None), loaded: Std::new(None), last_status: Std::new(None), dl_cancel: Std::new(None), gen_cancel: Std::new(None), busy_dl: AtomicBool::new(false), store: Std::new(None), last_perf: Std::new(None), tok_cache: Std::new(HashMap::new()) })
    }
    fn vault_path(&self) -> PathBuf { self.data.join("vault.json") }
    fn store_dir(&self) -> PathBuf { self.data.join("store") }
    fn with_store<T>(&self, f: impl FnOnce(&Store) -> veyra_store::Result<T>) -> Result<T, String> {
        let g = self.store.lock().unwrap_or_else(|e| e.into_inner()); let s = g.as_ref().ok_or("locked: unlock Veyra with the master password first")?; f(s).map_err(|e| e.to_string())
    }
}
fn now_s() -> i64 { std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0) }
fn s<E: std::fmt::Display>(e: E) -> String { e.to_string() }

/// Where the bundled engine lives: next to the app's resources (installed) or `VEYRA_LLAMA_SERVER` (development).
pub fn engine_binary(app: &AppHandle) -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("VEYRA_LLAMA_SERVER").map(PathBuf::from) { return p.is_file().then_some(p); }
    let exe = if cfg!(windows) { "llama-server.exe" } else { "llama-server" };
    let r = app.path().resource_dir().ok()?;
    [r.join("resources").join("llama").join(exe), r.join("llama").join(exe), r.join("_up_").join("resources").join("llama").join(exe)].into_iter().find(|p| p.is_file())
}

// ---------------------------------------------------------------- hardware, catalog, recommendation
#[derive(Serialize)] struct ModelView { #[serde(flatten)] spec: &'static ModelSpec, state: FileState, bytes_on_disk: u64, loaded: bool, recommended: bool, fits: bool }
#[derive(Serialize)] pub struct Overview { hardware: hardware::Hardware, free_disk_bytes: Option<u64>, engine_binary: bool, models: Vec<ModelView>, recommendation: Option<serde_json::Value>, status: veyra_ai::engine::Status, generating: bool, downloading: bool, last_perf: Option<Perf>, models_dir: String }

fn status_of(ai: &Ai) -> veyra_ai::engine::Status {
    ai.last_status.lock().unwrap_or_else(|e| e.into_inner()).clone().unwrap_or(veyra_ai::engine::Status { loaded: false, model: None, ctx: 0, threads: 0, rss_mib: None, load_ms: None })
}
#[tauri::command] pub async fn ai_overview(app: AppHandle, ai: A<'_>) -> Result<Overview, String> {
    let a = ai.inner().clone(); let bin = engine_binary(&app).is_some();
    spawn_blocking(move || {
        let hw = hardware::detect(); let rec = catalog::recommend(&hw); let loaded = a.loaded.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let total = hw.total_ram_mib as f64 / 1024.0;
        let models = catalog::CATALOG.iter().map(|m| { let (state, bytes) = installed::state(&a.models, m);
            ModelView { spec: m, state, bytes_on_disk: bytes, loaded: loaded.as_deref() == Some(m.id), recommended: rec.as_ref().map(|r| r.model.id == m.id).unwrap_or(false), fits: m.status == catalog::Status::Verified && total >= m.min_ram_gb + 2.0 } }).collect();
        Overview { free_disk_bytes: hardware::free_disk_bytes(&a.models), engine_binary: bin, models, recommendation: rec.map(|r| serde_json::json!({ "id": r.model.id, "reason": r.reason, "warnings": r.warnings })), status: status_of(&a), generating: a.gen_cancel.lock().unwrap_or_else(|e| e.into_inner()).is_some(), downloading: a.busy_dl.load(Ordering::Relaxed), last_perf: a.last_perf.lock().unwrap_or_else(|e| e.into_inner()).clone(), models_dir: a.models.display().to_string(), hardware: hw }
    }).await.map_err(s)
}

// ---------------------------------------------------------------- download / verify / delete
#[derive(Serialize, Clone)] struct DlEvent { id: String, phase: String, done: u64, total: Option<u64>, error: Option<String> }
fn emit_dl(app: &AppHandle, id: &str, phase: &str, done: u64, total: Option<u64>, error: Option<String>) { let _ = app.emit("ai-download", DlEvent { id: id.into(), phase: phase.into(), done, total, error }); }

#[tauri::command] pub async fn ai_download(app: AppHandle, ai: A<'_>, id: String) -> Result<(), String> {
    let spec = catalog::by_id(&id).filter(|m| m.status == catalog::Status::Verified).ok_or("unknown or unverified model")?;
    let a = ai.inner().clone(); if a.busy_dl.swap(true, Ordering::SeqCst) { return Err("a download is already running".into()); }
    let cancel = Arc::new(AtomicBool::new(false)); *a.dl_cancel.lock().unwrap_or_else(|e| e.into_inner()) = Some(cancel.clone());
    spawn(async move {
        let free = hardware::free_disk_bytes(&a.models);
        let ap = app.clone(); let idc = id.clone();
        let r = download::download(&a.net, &a.models, spec, None, free, &cancel, move |p| emit_dl(&ap, &idc, p.phase, p.done, p.total, None)).await;
        match r {
            Ok(_) => match installed::mark_verified(&a.models, spec) { Ok(_) => emit_dl(&app, &id, "verified", 0, None, None), Err(e) => emit_dl(&app, &id, "error", 0, None, Some(s(e))) },
            Err(download::DlError::Cancelled) => emit_dl(&app, &id, "cancelled", 0, None, None),
            Err(e) => emit_dl(&app, &id, "error", 0, None, Some(s(e))),
        }
        *a.dl_cancel.lock().unwrap_or_else(|e| e.into_inner()) = None; a.busy_dl.store(false, Ordering::SeqCst);
    });
    Ok(())
}
#[tauri::command] pub async fn ai_download_cancel(ai: A<'_>) -> Result<(), String> { if let Some(c) = ai.dl_cancel.lock().unwrap_or_else(|e| e.into_inner()).as_ref() { c.store(true, Ordering::Relaxed); } Ok(()) }

/// Re-hash a file that is already on disk (e.g. copied in by hand) and mark it verified only if the SHA-256 matches the catalog.
#[tauri::command] pub async fn ai_verify(app: AppHandle, ai: A<'_>, id: String) -> Result<bool, String> {
    let spec = catalog::by_id(&id).ok_or("unknown model")?; let exp = spec.sha256.ok_or("no verified hash for this model")?;
    let p = download::final_path(&ai.models, spec); if !p.exists() { return Err("model file not found".into()); }
    emit_dl(&app, &id, "verifying", 0, None, None);
    let got = download::sha256_file(&p, |_| {}).await.map_err(s)?;
    if got == exp { installed::mark_verified(&ai.models, spec).map_err(s)?; emit_dl(&app, &id, "verified", 0, None, None); Ok(true) } else { emit_dl(&app, &id, "error", 0, None, Some("SHA-256 does not match the catalog".into())); Ok(false) }
}
#[tauri::command] pub async fn ai_delete_model(ai: A<'_>, id: String) -> Result<(), String> {
    let spec = catalog::by_id(&id).ok_or("unknown model")?;
    if ai.loaded.lock().unwrap_or_else(|e| e.into_inner()).as_deref() == Some(spec.id) { return Err("unload the model first".into()); }
    installed::remove(&ai.models, spec).map_err(s)
}

// ---------------------------------------------------------------- engine
fn emit_engine(app: &AppHandle, ai: &Ai) { let _ = app.emit("ai-engine", status_of(ai)); }

/// Context window: 8192 tokens when there is room (KV cache ≈ 1.2 GB for a 4B model), 4096 otherwise. The user can override it.
fn default_ctx(h: &hardware::Hardware) -> u32 { if h.available_ram_mib >= 6 * 1024 { 8192 } else { 4096 } }

#[tauri::command] pub async fn ai_load(app: AppHandle, ai: A<'_>, id: String, ctx: Option<u32>, threads: Option<usize>) -> Result<(), String> {
    let spec = catalog::by_id(&id).filter(|m| m.status == catalog::Status::Verified).ok_or("unknown or unverified model")?;
    match installed::state(&ai.models, spec).0 { FileState::Verified => {}, FileState::Downloaded => return Err("the file is on disk but its SHA-256 has not been verified; run verification first".into()), _ => return Err("model not downloaded".into()) }
    let bin = engine_binary(&app).ok_or("the inference engine (llama-server) is missing from this installation; reinstall Veyra")?;
    let hw = hardware::detect();
    let cfg = EngineConfig { server_bin: bin.clone(), model: download::final_path(&ai.models, spec), ctx: ctx.unwrap_or_else(|| default_ctx(&hw)).clamp(1024, 32768), threads: threads.unwrap_or_else(|| hardware::auto_threads(&hw)).max(1), load_timeout: Duration::from_secs(300) };
    let mut g = ai.engine.lock().await;
    if let Some(old) = g.take() { old.unload().await; *ai.loaded.lock().unwrap_or_else(|e| e.into_inner()) = None; } // one model at a time
    veyra_ai::engine::reap_orphans(&ai.models, &bin);
    let mut e = Engine::load(ai.net.clone(), cfg).await.map_err(s)?;
    *ai.last_status.lock().unwrap_or_else(|e| e.into_inner()) = Some(e.status()); *ai.loaded.lock().unwrap_or_else(|e| e.into_inner()) = Some(id); *g = Some(e);
    drop(g); emit_engine(&app, &ai); Ok(())
}
#[tauri::command] pub async fn ai_unload(app: AppHandle, ai: A<'_>) -> Result<(), String> {
    if let Some(c) = ai.gen_cancel.lock().unwrap_or_else(|e| e.into_inner()).as_ref() { c.store(true, Ordering::Relaxed); }
    let mut g = ai.engine.lock().await; if let Some(e) = g.take() { e.unload().await; }
    *ai.loaded.lock().unwrap_or_else(|e| e.into_inner()) = None; *ai.last_status.lock().unwrap_or_else(|e| e.into_inner()) = None; drop(g); emit_engine(&app, &ai); Ok(())
}
/// Refreshes the cached status (RSS changes while generating) and reports a dead engine honestly.
#[tauri::command] pub async fn ai_status(ai: A<'_>) -> Result<veyra_ai::engine::Status, String> {
    if let Ok(mut g) = ai.engine.try_lock() { match g.as_mut() { Some(e) => { let st = e.status(); *ai.last_status.lock().unwrap_or_else(|e| e.into_inner()) = Some(st.clone()); if !st.loaded { *ai.loaded.lock().unwrap_or_else(|e| e.into_inner()) = None; } return Ok(st) } None => {} } }
    Ok(status_of(&ai))
}
#[tauri::command] pub async fn ai_cancel(ai: A<'_>) -> Result<(), String> { if let Some(c) = ai.gen_cancel.lock().unwrap_or_else(|e| e.into_inner()).as_ref() { c.store(true, Ordering::Relaxed); } Ok(()) }

#[derive(Serialize)] pub struct TestResult { text: String, perf: Perf, rss_mib: Option<u64> }
/// Real generation through the real engine: a short fixed prompt; what comes back is exactly what the model produced.
#[tauri::command] pub async fn ai_test(ai: A<'_>) -> Result<TestResult, String> {
    let cancel = Arc::new(AtomicBool::new(false)); { let mut c = ai.gen_cancel.lock().unwrap_or_else(|e| e.into_inner()); if c.is_some() { return Err("a generation is already running".into()); } *c = Some(cancel.clone()); }
    let r = async { let mut g = ai.engine.lock().await; let e = g.as_mut().ok_or("no model loaded")?;
        let msgs = vec![Msg { role: "user".into(), content: "Reply with one short sentence confirming you are running locally.".into() }];
        let (text, perf) = e.chat(&msgs, &GenParams { max_tokens: 64, temperature: 0.2, top_p: 0.9, thinking: false }, &cancel, |_| {}, |_| {}).await.map_err(s)?;
        let st = e.status(); *ai.last_status.lock().unwrap_or_else(|e| e.into_inner()) = Some(st.clone()); *ai.last_perf.lock().unwrap_or_else(|e| e.into_inner()) = Some(perf.clone());
        Ok::<_, String>(TestResult { text, perf, rss_mib: st.rss_mib }) }.await;
    *ai.gen_cancel.lock().unwrap_or_else(|e| e.into_inner()) = None; r
}

// ---------------------------------------------------------------- vault (master password) and conversations
#[derive(Serialize)] pub struct VaultStatus { exists: bool, unlocked: bool }
#[tauri::command] pub async fn vault_status(ai: A<'_>) -> Result<VaultStatus, String> { Ok(VaultStatus { exists: vault::exists(&ai.vault_path()), unlocked: ai.store.lock().unwrap_or_else(|e| e.into_inner()).is_some() }) }
fn finish_unlock(ai: &Ai, u: &vault::Unlocked) -> Result<(), String> {
    let st = Store::open(&ai.store_dir(), u).map_err(s)?;
    // a generation interrupted by a crash or quit leaves a pending message: mark it as an error instead of showing a frozen "typing" state
    for m in st.pending_messages().map_err(s)? { let _ = st.set_message(&m.id, &m.content, "error"); }
    *ai.store.lock().unwrap_or_else(|e| e.into_inner()) = Some(st); Ok(())
}
/// Creates the vault and returns the recovery key (shown once; it is the only way back in if the password is lost).
#[tauri::command] pub async fn vault_create(ai: A<'_>, password: String) -> Result<String, String> {
    let a = ai.inner().clone();
    spawn_blocking(move || { let (u, rk) = vault::create(&a.vault_path(), &password, KdfParams::DEFAULT).map_err(s)?; finish_unlock(&a, &u)?; Ok(rk) }).await.map_err(s)?
}
#[tauri::command] pub async fn vault_unlock(ai: A<'_>, password: String) -> Result<(), String> {
    let a = ai.inner().clone(); spawn_blocking(move || { let u = vault::unlock(&a.vault_path(), &password).map_err(s)?; finish_unlock(&a, &u) }).await.map_err(s)?
}
#[tauri::command] pub async fn vault_unlock_recovery(ai: A<'_>, recovery: String) -> Result<(), String> {
    let a = ai.inner().clone(); spawn_blocking(move || { let u = vault::unlock_with_recovery(&a.vault_path(), &recovery).map_err(s)?; finish_unlock(&a, &u) }).await.map_err(s)?
}
#[tauri::command] pub async fn vault_lock(ai: A<'_>) -> Result<(), String> { if ai.gen_cancel.lock().unwrap_or_else(|e| e.into_inner()).is_some() { return Err("a generation is running".into()); } *ai.store.lock().unwrap_or_else(|e| e.into_inner()) = None; Ok(()) }

#[tauri::command] pub async fn conv_list(ai: A<'_>, view: String, query: Option<String>) -> Result<Vec<Conversation>, String> {
    let v = match view.as_str() { "archived" => View::Archived, "trash" => View::Trash, "favorites" => View::Favorites, _ => View::Active };
    ai.with_store(|st| match query.as_deref().map(str::trim).filter(|q| !q.is_empty()) { Some(q) => st.search(q), None => st.list(v, None) })
}
#[tauri::command] pub async fn conv_messages(ai: A<'_>, id: String) -> Result<Vec<Message>, String> { ai.with_store(|st| st.messages(&id)) }
#[tauri::command] pub async fn conv_create(ai: A<'_>) -> Result<Conversation, String> { ai.with_store(|st| st.create_conversation("New conversation", now_s())) }
#[tauri::command] pub async fn conv_rename(ai: A<'_>, id: String, title: String) -> Result<(), String> { ai.with_store(|st| st.rename(&id, &title)) }
#[tauri::command] pub async fn conv_archive(ai: A<'_>, id: String, archived: bool) -> Result<(), String> { ai.with_store(|st| st.set_archived(&id, archived)) }
#[tauri::command] pub async fn conv_trash(ai: A<'_>, id: String) -> Result<(), String> { ai.with_store(|st| st.trash(&id, now_s())) }
#[tauri::command] pub async fn conv_restore(ai: A<'_>, id: String) -> Result<(), String> { ai.with_store(|st| st.restore(&id)) }
#[tauri::command] pub async fn conv_delete_permanently(ai: A<'_>, id: String) -> Result<(), String> { ai.with_store(|st| st.delete_permanently(&id)) }

// ---------------------------------------------------------------- chat
#[derive(Serialize)] pub struct Sent { conversation: Conversation, user_message: Message, assistant_message: Message }
#[derive(Serialize, Clone)] struct Token { conversation: String, message: String, delta: String }
#[derive(Serialize, Clone)] struct Done { conversation: String, message: String, perf: Perf, cancelled: bool, context_messages: usize, total_messages: usize, summarised: bool }
#[derive(Serialize, Clone)] struct ChatErr { conversation: String, message: String, error: String }

const SYSTEM_PROMPT: &str = "You are Veyra, a private assistant running entirely on the user's own computer. Answer in the language the user writes in. Be accurate and concise; use Markdown, and fenced code blocks with a language tag for code. If you do not know something or cannot do it, say so plainly instead of guessing.";

fn hash(s: &str) -> u64 { use std::hash::{Hash, Hasher}; let mut h = std::collections::hash_map::DefaultHasher::new(); s.hash(&mut h); h.finish() }

/// Sends a user message and starts generating the reply in the background. Returns immediately with the stored messages; text arrives as events.
#[tauri::command] pub async fn chat_send(app: AppHandle, ai: A<'_>, conversation: Option<String>, text: String, thinking: Option<bool>) -> Result<Sent, String> {
    let text = text.trim().to_string(); if text.is_empty() { return Err("empty message".into()); }
    if ai.loaded.lock().unwrap_or_else(|e| e.into_inner()).is_none() { return Err("no model is loaded: open Settings → Artificial intelligence and load one".into()); }
    let cancel = Arc::new(AtomicBool::new(false));
    { let mut c = ai.gen_cancel.lock().unwrap_or_else(|e| e.into_inner()); if c.is_some() { return Err("a reply is already being generated".into()); } *c = Some(cancel.clone()); }
    let prep = ai.with_store(|st| {
        let conv = match &conversation { Some(id) => st.conversation(id)?, None => st.create_conversation("New conversation", now_s())? };
        let first = st.messages(&conv.id)?.is_empty();
        let um = st.add_message(&conv.id, "user", &text, "sent", now_s())?;
        if first { let t: String = text.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(48).collect(); st.rename(&conv.id, &t)?; }
        let am = st.add_message(&conv.id, "assistant", "", "pending", now_s())?;
        Ok((st.conversation(&conv.id)?, um, am))
    });
    let (conv, um, am) = match prep { Ok(x) => x, Err(e) => { *ai.gen_cancel.lock().unwrap_or_else(|e| e.into_inner()) = None; return Err(e); } };
    let a = ai.inner().clone(); let (cid, mid) = (conv.id.clone(), am.id.clone());
    spawn(async move {
        let r = generate(&app, &a, &cid, &mid, thinking.unwrap_or(false), &cancel).await;
        if let Err(e) = r {  let _ = app.emit("chat-error", ChatErr { conversation: cid.clone(), message: mid.clone(), error: e }); }
        *a.gen_cancel.lock().unwrap_or_else(|e| e.into_inner()) = None; emit_engine(&app, &a);
    });
    Ok(Sent { conversation: conv, user_message: um, assistant_message: am })
}

async fn generate(app: &AppHandle, a: &Arc<Ai>, cid: &str, mid: &str, thinking: bool, cancel: &Arc<AtomicBool>) -> Result<(), String> {
    let mut g = a.engine.lock().await; let eng = g.as_mut().ok_or("the model was unloaded")?;
    let all: Vec<Message> = a.with_store(|st| st.messages(cid))?;
    let history: Vec<Msg> = all.iter().filter(|m| m.id != mid && !(m.role == "assistant" && (m.status == "error" && m.content.is_empty()))).map(|m| Msg { role: m.role.clone(), content: m.content.clone() }).collect();
    let ctx = eng.status().ctx as usize; let reserve = 1024usize.min(ctx / 4); let budget = ctx.saturating_sub(reserve);
    // exact token counts from the loaded model's own tokenizer, cached by content
    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut need: Vec<&str> = history.iter().map(|m| m.content.as_str()).collect(); need.push(SYSTEM_PROMPT);
    let stored_summary = a.with_store(|st| Ok(st.conversation(cid)?.summary))?; let (covers, summary_text) = context::decode_summary(stored_summary.as_deref());
    if let Some(t) = &summary_text { need.push(t); }
    for t in need { if counts.contains_key(t) { continue; } let h = hash(t); let cached = a.tok_cache.lock().unwrap_or_else(|e| e.into_inner()).get(&h).copied();
        let n = match cached { Some(n) => n, None => { let n = eng.count_tokens(t).await.map_err(s)?; a.tok_cache.lock().unwrap_or_else(|e| e.into_inner()).insert(h, n); n } }; counts.insert(t.to_string(), n); }
    let count = |t: &str| counts.get(t).copied().unwrap_or_else(|| t.len() / 3 + 1); // fallback only for the "Summary of…" wrapper text
    let mut built = context::build(SYSTEM_PROMPT, summary_text.as_deref(), &history, budget, &count);
    let mut summarised = false;
    if built.first_included > covers {
        // turns older than the verbatim window are not covered by the stored summary yet: extend it (merge old summary + newly dropped turns), then rebuild
        let newly = &history[covers.min(built.first_included)..built.first_included];
        let req = context::summary_request(summary_text.as_deref(), newly);
        let (txt, _) = eng.chat(&req, &GenParams { max_tokens: 400, temperature: 0.2, top_p: 0.9, thinking: false }, cancel, |_| {}, |_| {}).await.map_err(|e| format!("summarising the older messages failed: {e}"))?;
        let enc = context::encode_summary(built.first_included, &txt); a.with_store(|st| st.set_summary(cid, &enc))?; summarised = true;
        let n = eng.count_tokens(&txt).await.unwrap_or(txt.len() / 3); counts.insert(txt.clone(), n);
        let count2 = |t: &str| counts.get(t).copied().unwrap_or_else(|| t.len() / 3 + 1);
        built = context::build(SYSTEM_PROMPT, Some(&txt), &history, budget, &count2);
    }
    // Live streaming: every piece goes to the UI the moment the engine produces it; the partial answer is saved every 2 s so a crash loses little.
    let mut acc = String::new(); let mut last_save = Instant::now(); let (appc, cc, mc, ac) = (app.clone(), cid.to_string(), mid.to_string(), a.clone());
    let res = eng.chat(&built.messages, &GenParams { thinking, ..GenParams::default() }, cancel, |d| {
        acc.push_str(d); let _ = appc.emit("chat-token", Token { conversation: cc.clone(), message: mc.clone(), delta: d.to_string() });
        if last_save.elapsed() > Duration::from_secs(2) { let _ = ac.with_store(|st| st.set_message(&mc, &acc, "pending")); last_save = Instant::now(); }
    }, |_| {}).await;
    let total = all.len();
    match res {
        Ok((text, perf)) => {
            a.with_store(|st| st.set_message(mid, &text, "sent"))?; *a.last_perf.lock().unwrap_or_else(|e| e.into_inner()) = Some(perf.clone()); *a.last_status.lock().unwrap_or_else(|e| e.into_inner()) = Some(eng.status());
            let _ = app.emit("chat-done", Done { conversation: cid.into(), message: mid.into(), perf, cancelled: false, context_messages: built.messages.len(), total_messages: total, summarised }); Ok(())
        }
        Err(EngineError::Cancelled) => {
            a.with_store(|st| st.set_message(mid, &acc, "sent"))?; // what was produced before the stop is kept
            let _ = app.emit("chat-done", Done { conversation: cid.into(), message: mid.into(), perf: Perf::default(), cancelled: true, context_messages: built.messages.len(), total_messages: total, summarised }); Ok(())
        }
        Err(e) => { let _ = a.with_store(|st| st.set_message(mid, &acc, "error")); Err(s(e)) }
    }
}

/// Called on exit: stop generation and end the engine process (also the pidfile logic covers a crash that skips this).
pub async fn shutdown(ai: &Arc<Ai>) {
    if let Some(c) = ai.gen_cancel.lock().unwrap_or_else(|e| e.into_inner()).as_ref() { c.store(true, Ordering::Relaxed); }
    if let Some(c) = ai.dl_cancel.lock().unwrap_or_else(|e| e.into_inner()).as_ref() { c.store(true, Ordering::Relaxed); }
    let mut g = ai.engine.lock().await; if let Some(e) = g.take() { e.unload().await; }
}
