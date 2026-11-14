//! Tauri layer over `veyra-ai` / `veyra-net` / `veyra-store`. The inference engine runs INSIDE this process (libllama linked statically): there is
//! no child process and no local server. Every command is `async` (never blocks the UI thread; window-creating commands must be async on Windows);
//! blocking native work (model load, generation, hashing) runs on worker threads and reports through events that EVERY window receives, which is
//! what makes the chat window and the full application share one state:
//!   `ai-download` {id, phase, done, total, error}   `ai-state` {status, loading, error, selected}
//!   `chat-token` {conversation, message, delta}      `chat-done` {…}   `chat-error` {…}      `store-ready` {…}
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex as Std};
use std::time::{Duration, Instant};
use tauri::async_runtime::{spawn, spawn_blocking};
use tauri::{AppHandle, Emitter, State};
use veyra_ai::catalog::{self, ModelSpec};
use veyra_ai::context::{self, Msg};
use veyra_ai::download;
use veyra_ai::engine::{Engine, EngineConfig, EngineError, GenParams, Perf, Status};
use veyra_ai::hardware;
use veyra_ai::installed::{self, FileState};
use veyra_net::{Config, NetClient};
use veyra_store::crypto::KdfParams;
use veyra_store::db::{Conversation, Message, Store, View};
use veyra_store::system_key::{self, KeyStore};

pub struct Ai {
    pub net: NetClient, pub data: PathBuf, models: PathBuf,
    engine: Std<Option<Engine>>, loaded: Std<Option<String>>, base_status: Std<Option<Status>>, loading: AtomicBool, last_error: Std<Option<String>>,
    dl_cancel: Std<Option<Arc<AtomicBool>>>, gen_cancel: Std<Option<Arc<AtomicBool>>>, busy_dl: AtomicBool, last_activity: AtomicU64,
    store: Std<Option<Store>>, key_kind: Std<&'static str>, store_error: Std<Option<String>>, last_perf: Std<Option<Perf>>, tok_cache: Std<HashMap<u64, usize>>,
}
type A<'a> = State<'a, Arc<Ai>>;
fn lk<T>(m: &Std<T>) -> std::sync::MutexGuard<'_, T> { m.lock().unwrap_or_else(|e| e.into_inner()) }

impl Ai {
    pub fn new(data: PathBuf) -> Result<Ai, String> {
        let net = NetClient::new(Config::default()).map_err(|e| format!("network gateway: {e:?}"))?;
        let models = data.join("models"); let _ = std::fs::create_dir_all(&models);
        Ok(Ai { net, data, models, engine: Std::new(None), loaded: Std::new(None), base_status: Std::new(None), loading: AtomicBool::new(false), last_error: Std::new(None), dl_cancel: Std::new(None), gen_cancel: Std::new(None), busy_dl: AtomicBool::new(false), last_activity: AtomicU64::new(now_s() as u64),
            store: Std::new(None), key_kind: Std::new("none"), store_error: Std::new(None), last_perf: Std::new(None), tok_cache: Std::new(HashMap::new()) })
    }
    fn store_dir(&self) -> PathBuf { self.data.join("store") }
    fn with_store<T>(&self, f: impl FnOnce(&Store) -> veyra_store::Result<T>) -> Result<T, String> {
        let g = lk(&self.store); let s = g.as_ref().ok_or_else(|| lk(&self.store_error).clone().unwrap_or_else(|| "the encrypted storage is not ready yet".into()))?; f(s).map_err(|e| e.to_string())
    }
    fn touch(&self) { self.last_activity.store(now_s() as u64, Ordering::Relaxed); }
}
fn now_s() -> i64 { std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0) }
fn s<E: std::fmt::Display>(e: E) -> String { e.to_string() }

// ---------------------------------------------------------------- state shared by every window
#[derive(Serialize, Clone)] pub struct EngineState { status: Status, loading: bool, error: Option<String>, selected: Option<String>, generating: bool, downloading: bool, stub: bool }
fn state_of(app: &AppHandle, ai: &Ai) -> EngineState {
    let mut st = lk(&ai.base_status).clone().unwrap_or(Status { loaded: false, model: None, ctx: 0, threads: 0, rss_mib: None, load_ms: None, in_process: true, llama: veyra_llama::version() });
    if st.loaded { st.rss_mib = veyra_ai::engine::own_rss_mib(); }
    EngineState { status: st, loading: ai.loading.load(Ordering::Relaxed), error: lk(&ai.last_error).clone(), selected: crate::settings(app).ai.model_id, generating: lk(&ai.gen_cancel).is_some(), downloading: ai.busy_dl.load(Ordering::Relaxed), stub: veyra_llama::IS_STUB }
}
fn emit_state(app: &AppHandle, ai: &Ai) { let _ = app.emit("ai-state", state_of(app, ai)); }
#[tauri::command] pub async fn ai_state(app: AppHandle, ai: A<'_>) -> Result<EngineState, String> { Ok(state_of(&app, &ai)) }

// ---------------------------------------------------------------- hardware, catalog, recommendation
#[derive(Serialize)] struct ModelView { #[serde(flatten)] spec: &'static ModelSpec, state: FileState, bytes_on_disk: u64, loaded: bool, recommended: bool, fits: bool }
#[derive(Serialize)] pub struct Overview { hardware: hardware::Hardware, free_disk_bytes: Option<u64>, models: Vec<ModelView>, recommendation: Option<serde_json::Value>, engine: EngineState, last_perf: Option<Perf>, models_dir: String }
#[tauri::command] pub async fn ai_overview(app: AppHandle, ai: A<'_>) -> Result<Overview, String> {
    let a = ai.inner().clone();
    spawn_blocking(move || {
        let hw = hardware::detect(); let rec = catalog::recommend(&hw); let loaded = lk(&a.loaded).clone(); let total = hw.total_ram_mib as f64 / 1024.0;
        let models = catalog::CATALOG.iter().map(|m| { let (state, bytes) = installed::state(&a.models, m);
            ModelView { spec: m, state, bytes_on_disk: bytes, loaded: loaded.as_deref() == Some(m.id), recommended: rec.as_ref().map(|r| r.model.id == m.id).unwrap_or(false), fits: m.status == catalog::Status::Verified && total >= m.min_ram_gb + 2.0 } }).collect();
        Overview { free_disk_bytes: hardware::free_disk_bytes(&a.models), models, recommendation: rec.map(|r| serde_json::json!({ "id": r.model.id, "reason": r.reason, "warnings": r.warnings })), engine: state_of(&app, &a), last_perf: lk(&a.last_perf).clone(), models_dir: a.models.display().to_string(), hardware: hw }
    }).await.map_err(s)
}

// ---------------------------------------------------------------- download / verify / delete
#[derive(Serialize, Clone)] struct DlEvent { id: String, phase: String, done: u64, total: Option<u64>, error: Option<String> }
fn emit_dl(app: &AppHandle, id: &str, phase: &str, done: u64, total: Option<u64>, error: Option<String>) { let _ = app.emit("ai-download", DlEvent { id: id.into(), phase: phase.into(), done, total, error }); }

#[tauri::command] pub async fn ai_download(app: AppHandle, ai: A<'_>, id: String) -> Result<(), String> {
    let spec = catalog::by_id(&id).filter(|m| m.status == catalog::Status::Verified).ok_or("unknown or unverified model")?;
    let a = ai.inner().clone(); if a.busy_dl.swap(true, Ordering::SeqCst) { return Err("a download is already running".into()); }
    let cancel = Arc::new(AtomicBool::new(false)); *lk(&a.dl_cancel) = Some(cancel.clone()); let app2 = app.clone();
    spawn(async move { let app = app2;
        let free = hardware::free_disk_bytes(&a.models); let ap = app.clone(); let idc = id.clone();
        let r = download::download(&a.net, &a.models, spec, None, free, &cancel, move |p| emit_dl(&ap, &idc, p.phase, p.done, p.total, None)).await;
        match r {
            Ok(_) => match installed::mark_verified(&a.models, spec) { Ok(_) => emit_dl(&app, &id, "verified", 0, None, None), Err(e) => emit_dl(&app, &id, "error", 0, None, Some(s(e))) },
            Err(download::DlError::Cancelled) => emit_dl(&app, &id, "cancelled", 0, None, None),
            Err(e) => emit_dl(&app, &id, "error", 0, None, Some(s(e))),
        }
        *lk(&a.dl_cancel) = None; a.busy_dl.store(false, Ordering::SeqCst); emit_state(&app, &a);
    });
    emit_state(&app, &ai); Ok(())
}
#[tauri::command] pub async fn ai_download_cancel(ai: A<'_>) -> Result<(), String> { if let Some(c) = lk(&ai.dl_cancel).as_ref() { c.store(true, Ordering::Relaxed); } Ok(()) }
#[tauri::command] pub async fn ai_verify(app: AppHandle, ai: A<'_>, id: String) -> Result<bool, String> {
    let spec = catalog::by_id(&id).ok_or("unknown model")?; let exp = spec.sha256.ok_or("no verified hash for this model")?;
    let p = download::final_path(&ai.models, spec); if !p.exists() { return Err("model file not found".into()); }
    emit_dl(&app, &id, "verifying", 0, None, None);
    let got = download::sha256_file(&p, |_| {}).await.map_err(s)?;
    if got == exp { installed::mark_verified(&ai.models, spec).map_err(s)?; emit_dl(&app, &id, "verified", 0, None, None); Ok(true) } else { emit_dl(&app, &id, "error", 0, None, Some("SHA-256 does not match the catalog".into())); Ok(false) }
}
#[tauri::command] pub async fn ai_delete_model(app: AppHandle, ai: A<'_>, id: String) -> Result<(), String> {
    let spec = catalog::by_id(&id).ok_or("unknown model")?;
    if lk(&ai.loaded).as_deref() == Some(spec.id) { return Err("unload the model first".into()); }
    installed::remove(&ai.models, spec).map_err(s)?; emit_state(&app, &ai); Ok(())
}

// ---------------------------------------------------------------- engine (in this process)
fn default_ctx(h: &hardware::Hardware) -> u32 { if h.apple_silicon && h.available_ram_mib >= 6 * 1024 { 8192 } else { 4096 } }
/// "Not enough memory" and friends get an action the user can take, not a raw native message.
fn friendly(e: &EngineError) -> String {
    let m = e.to_string(); let l = m.to_lowercase();
    if l.contains("memory") || l.contains("alloc") { format!("{m}\nTry a smaller context size (Settings → Artificial intelligence), a smaller model such as Qwen3 4B Q4_K_M, or close other applications.") }
    else if l.contains("cannot load the model") { format!("{m}\nThe file may be damaged: open Models, delete it and download it again.") } else { m }
}

/// Loads `id` (replacing any loaded model) and remembers it as the selected model. Reuses nothing: callers check `loaded` first.
pub async fn load_model(app: &AppHandle, ai: &Arc<Ai>, id: &str) -> Result<(), String> {
    let spec = catalog::by_id(id).filter(|m| m.status == catalog::Status::Verified).ok_or("unknown or unverified model")?;
    match installed::state(&ai.models, spec).0 { FileState::Verified => {}, FileState::Downloaded => return Err("the file is on disk but its SHA-256 has not been verified; run verification first".into()), _ => return Err("model not downloaded".into()) }
    if veyra_llama::IS_STUB { return Err("this build of Veyra does not include the inference library".into()); }
    if ai.loading.swap(true, Ordering::SeqCst) { return Err("a model is already being loaded".into()); }
    *lk(&ai.last_error) = None; if let Some(c) = lk(&ai.gen_cancel).as_ref() { c.store(true, Ordering::Relaxed); } emit_state(app, ai);
    let cfg = engine_config(app, &download::final_path(&ai.models, spec)); let a = ai.clone(); let ids = id.to_string();
    let r = spawn_blocking(move || {
        let mut g = lk(&a.engine); if let Some(old) = g.take() { old.unload(); *lk(&a.loaded) = None; *lk(&a.base_status) = None; } // one model at a time; memory freed before the next load
        match Engine::load(cfg) { Ok(e) => { *lk(&a.base_status) = Some(e.status()); *lk(&a.loaded) = Some(ids); *g = Some(e); Ok(()) } Err(e) => Err(friendly(&e)) }
    }).await.map_err(s).and_then(|x| x);
    ai.loading.store(false, Ordering::SeqCst); ai.touch();
    match &r { Ok(_) => { let mut st = crate::settings(app); if st.ai.model_id.as_deref() != Some(id) { st.ai.model_id = Some(id.to_string()); let _ = crate::save(app, st); } }, Err(e) => *lk(&ai.last_error) = Some(e.clone()) }
    emit_state(app, ai); r
}
fn engine_config(app: &AppHandle, model: &std::path::Path) -> EngineConfig {
    let hw = hardware::detect(); let st = crate::settings(app).ai;
    let mut c = EngineConfig::new(model.to_path_buf(), st.ctx.unwrap_or_else(|| default_ctx(&hw)), st.threads.map(|t| t as usize).unwrap_or_else(|| hardware::auto_threads(&hw)));
    c.batch = st.batch; c.ubatch = st.batch.min(512); c.kv_q8 = st.kv_cache_q8; c.flash_attn = match st.flash_attention.as_str() { "on" => 1, "off" => 0, _ => -1 };
    if hw.apple_silicon { c.gpu_layers = -1; } c
}
#[tauri::command] pub async fn ai_load(app: AppHandle, ai: A<'_>, id: String) -> Result<(), String> { let a = ai.inner().clone(); load_model(&app, &a, &id).await }
pub async fn unload_model(app: &AppHandle, ai: &Arc<Ai>) {
    if let Some(c) = lk(&ai.gen_cancel).as_ref() { c.store(true, Ordering::Relaxed); }
    let a = ai.clone(); let _ = spawn_blocking(move || { let mut g = lk(&a.engine); if let Some(e) = g.take() { e.unload(); } *lk(&a.loaded) = None; *lk(&a.base_status) = None; }).await;
    emit_state(app, ai);
}
#[tauri::command] pub async fn ai_unload(app: AppHandle, ai: A<'_>) -> Result<(), String> { let a = ai.inner().clone(); unload_model(&app, &a).await; Ok(()) }
#[tauri::command] pub async fn ai_cancel(ai: A<'_>) -> Result<(), String> { if let Some(c) = lk(&ai.gen_cancel).as_ref() { c.store(true, Ordering::Relaxed); } Ok(()) }

/// At start: load the selected model if it is downloaded, verified and auto-load is on. Never blocks start-up; failures surface as a state, not a crash.
pub fn auto_load(app: &AppHandle, ai: &Arc<Ai>) {
    let (app, ai) = (app.clone(), ai.clone());
    spawn(async move {
        let st = crate::settings(&app).ai; let Some(id) = st.model_id.clone() else { return }; if !st.auto_load || veyra_llama::IS_STUB { return }
        let Some(spec) = catalog::by_id(&id) else { return }; if installed::state(&ai.models, spec).0 != FileState::Verified { return }
        let t = Instant::now(); let r = load_model(&app, &ai, &id).await; crate::log(&app, &format!("auto-load {id}: {:?} in {} ms", r.as_ref().map(|_| "ok").map_err(|e| e.lines().next().unwrap_or("").to_string()), t.elapsed().as_millis()));
    });
}
/// Optional idle unload (off by default): checked every 20 s.
pub fn start_idle_watcher(app: &AppHandle, ai: &Arc<Ai>) {
    let (app, ai) = (app.clone(), ai.clone());
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(20));
        let Some(min) = crate::settings(&app).ai.idle_unload_minutes else { continue };
        if lk(&ai.loaded).is_none() || lk(&ai.gen_cancel).is_some() || ai.loading.load(Ordering::Relaxed) { continue }
        if (now_s() as u64).saturating_sub(ai.last_activity.load(Ordering::Relaxed)) >= min as u64 * 60 {
            let (a2, ai2) = (app.clone(), ai.clone()); tauri::async_runtime::block_on(async move { unload_model(&a2, &ai2).await; }); crate::log(&app, "model unloaded after the configured idle time");
        }
    });
}

#[derive(Serialize)] pub struct TestResult { text: String, perf: Perf, rss_mib: Option<u64> }
/// Real generation through the real engine: a short fixed prompt; what comes back is exactly what the model produced.
#[tauri::command] pub async fn ai_test(ai: A<'_>) -> Result<TestResult, String> {
    let cancel = Arc::new(AtomicBool::new(false)); { let mut c = lk(&ai.gen_cancel); if c.is_some() { return Err("a generation is already running".into()); } *c = Some(cancel.clone()); }
    let a = ai.inner().clone(); let c2 = cancel.clone();
    let r = spawn_blocking(move || { let mut g = lk(&a.engine); let e = g.as_mut().ok_or("no model is loaded")?;
        let msgs = vec![Msg { role: "user".into(), content: "Reply with one short sentence confirming you are running locally.".into() }];
        let (text, perf) = e.chat(&msgs, &GenParams { max_tokens: 64, temperature: 0.2, top_p: 0.9, top_k: 20, thinking: false }, &c2, |_| {}, |_| {}).map_err(|e| e.to_string())?;
        *lk(&a.last_perf) = Some(perf.clone()); Ok::<_, String>(TestResult { text, perf, rss_mib: veyra_ai::engine::own_rss_mib() }) }).await.map_err(s).and_then(|x| x);
    *lk(&ai.gen_cancel) = None; ai.touch(); r
}

// ---------------------------------------------------------------- encrypted storage with a system-protected key (no password)
/// Opens (or creates) the encrypted store. Called once at start; the key never leaves the OS credential store except in memory.
pub fn open_store(app: &AppHandle, ai: &Arc<Ai>) {
    let ks: Box<dyn KeyStore> = system_key::default_store("app.veyra.desktop", &ai.data.join("keys").join("data.key"));
    // earlier development builds protected the data with a master password; that data cannot be opened without it and there is no migration (no release ever shipped it)
    let legacy = ai.data.join("vault.json");
    if legacy.is_file() { let dst = ai.data.join(format!("legacy-password-vault-{}", now_s())); let _ = std::fs::create_dir_all(&dst); let _ = std::fs::rename(&legacy, dst.join("vault.json")); let _ = std::fs::rename(ai.store_dir(), dst.join("store")); crate::log(app, "moved a legacy password-protected vault aside"); }
    let r = system_key::open_or_create(ks.as_ref()).and_then(|(u, created)| Store::open(&ai.store_dir(), &u).map(|st| (st, created)));
    match r {
        Ok((st, created)) => { for m in st.pending_messages().unwrap_or_default() { let _ = st.set_message(&m.id, &m.content, "error"); } // replies interrupted by a crash or quit
            *lk(&ai.store) = Some(st); *lk(&ai.key_kind) = ks.kind(); crate::log(app, &format!("encrypted storage ready (key protected by: {}{})", ks.kind(), if created { ", newly created" } else { "" })); }
        Err(e) => { *lk(&ai.store_error) = Some(format!("The encrypted storage could not be opened: {e}. Your key is stored by the operating system for your account; if you restored files from another computer, import your key export in Settings → Security.")); crate::log(app, &format!("storage error: {e}")); }
    }
    let _ = app.emit("store-ready", ());
}
#[derive(Serialize)] pub struct SecurityStatus { ready: bool, error: Option<String>, key_protection: String, key_kind: String, data_dir: String, setup_done: bool }
#[tauri::command] pub async fn security_status(app: AppHandle, ai: A<'_>) -> Result<SecurityStatus, String> {
    let kind = *lk(&ai.key_kind);
    let human = match (kind, cfg!(windows), cfg!(target_os = "macos")) { ("os", true, _) => "Windows Credential Manager (protected by DPAPI, tied to your Windows account)", ("os", _, true) => "macOS Keychain (tied to your user account)", ("file", _, _) => "a file readable only by your user account (the operating system's credential store is not available here; weaker protection)", _ => "not available" };
    Ok(SecurityStatus { ready: lk(&ai.store).is_some(), error: lk(&ai.store_error).clone(), key_protection: human.into(), key_kind: kind.into(), data_dir: ai.data.display().to_string(), setup_done: crate::settings(&app).setup_done })
}
/// Writes a passphrase-protected export of the data key next to the backups. Needed only to restore on ANOTHER computer; never asked at start-up.
#[tauri::command] pub async fn key_export(ai: A<'_>, passphrase: String) -> Result<String, String> {
    let a = ai.inner().clone();
    spawn_blocking(move || { let ks = system_key::default_store("app.veyra.desktop", &a.data.join("keys").join("data.key")); let (u, _) = system_key::open_or_create(ks.as_ref()).map_err(s)?;
        let bundle = system_key::export_key(&u, &passphrase, KdfParams::DEFAULT).map_err(s)?; let dir = a.data.join("exports"); std::fs::create_dir_all(&dir).map_err(s)?;
        let p = dir.join(format!("veyra-key-export-{}.json", now_s())); std::fs::write(&p, bundle).map_err(s)?; Ok(p.display().to_string()) }).await.map_err(s)?
}
#[derive(Serialize)] pub struct BackupInfo { name: String, bytes: u64, modified: i64 }
#[tauri::command] pub async fn backup_create(ai: A<'_>) -> Result<String, String> {
    let a = ai.inner().clone(); spawn_blocking(move || { let dir = a.data.join("backups"); let p = a.with_store(|st| veyra_store::backup::create(st, &a.data, &dir, now_s()))?;
        let _ = veyra_store::backup::prune(&dir, now_s(), 90, 20, 2 << 30); Ok(p.display().to_string()) }).await.map_err(s)?
}
#[tauri::command] pub async fn backup_list(ai: A<'_>) -> Result<Vec<BackupInfo>, String> {
    let dir = ai.data.join("backups"); let mut v = vec![];
    if let Ok(rd) = std::fs::read_dir(&dir) { for e in rd.flatten() { if let Ok(m) = e.metadata() { let size = if m.is_dir() { dir_size(&e.path()) } else { m.len() }; let t = m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs() as i64).unwrap_or(0); v.push(BackupInfo { name: e.file_name().to_string_lossy().into_owned(), bytes: size, modified: t }); } } }
    v.sort_by(|a, b| b.modified.cmp(&a.modified)); Ok(v)
}
fn dir_size(p: &std::path::Path) -> u64 { std::fs::read_dir(p).map(|rd| rd.flatten().map(|e| e.metadata().map(|m| if m.is_dir() { dir_size(&e.path()) } else { m.len() }).unwrap_or(0)).sum()).unwrap_or(0) }

/// First-run installation step. `Veyra@2024` is an installation/configuration key chosen by the project owner. It is compared by SHA-256 only to avoid storing it in
/// clear, but any universal key shipped inside an application can be extracted: it is NOT a licence system and NOT a cryptographic protection, and it is never used to
/// derive or wrap any data key (the data key is random and lives in the OS credential store). See docs/SECURITY.md.
const INSTALL_KEY_SHA256: &str = "42a07a1d42860d921be7ecf6b3e56e55eae1b9dfc75035fff9140094f4e6be8b";
fn install_key_ok(input: &str) -> bool { veyra_store::crypto::sha256_hex(input.trim().as_bytes()) == INSTALL_KEY_SHA256 }
#[tauri::command] pub async fn setup_complete(app: AppHandle, key: String) -> Result<(), String> {
    if !install_key_ok(&key) { return Err("This installation key is not valid.".into()); }
    let mut st = crate::settings(&app); st.setup_done = true; crate::save(&app, st)
}

// ---------------------------------------------------------------- conversations
#[tauri::command] pub async fn conv_list(ai: A<'_>, view: String, query: Option<String>) -> Result<Vec<Conversation>, String> {
    let v = match view.as_str() { "archived" => View::Archived, "trash" => View::Trash, "favorites" => View::Favorites, _ => View::Active };
    ai.with_store(|st| match query.as_deref().map(str::trim).filter(|q| !q.is_empty()) { Some(q) => st.search(q), None => st.list(v, None) })
}
#[tauri::command] pub async fn conv_messages(ai: A<'_>, id: String) -> Result<Vec<Message>, String> { ai.with_store(|st| st.messages(&id)) }
#[tauri::command] pub async fn conv_create(ai: A<'_>) -> Result<Conversation, String> { ai.with_store(|st| st.create_conversation("New conversation", now_s())) }
#[tauri::command] pub async fn conv_rename(app: AppHandle, ai: A<'_>, id: String, title: String) -> Result<(), String> { ai.with_store(|st| st.rename(&id, &title))?; let _ = app.emit("conversations-changed", ()); Ok(()) }
#[tauri::command] pub async fn conv_archive(app: AppHandle, ai: A<'_>, id: String, archived: bool) -> Result<(), String> { ai.with_store(|st| st.set_archived(&id, archived))?; let _ = app.emit("conversations-changed", ()); Ok(()) }
#[tauri::command] pub async fn conv_trash(app: AppHandle, ai: A<'_>, id: String) -> Result<(), String> { ai.with_store(|st| st.trash(&id, now_s()))?; let _ = app.emit("conversations-changed", ()); Ok(()) }
#[tauri::command] pub async fn conv_restore(app: AppHandle, ai: A<'_>, id: String) -> Result<(), String> { ai.with_store(|st| st.restore(&id))?; let _ = app.emit("conversations-changed", ()); Ok(()) }
#[tauri::command] pub async fn conv_delete_permanently(app: AppHandle, ai: A<'_>, id: String) -> Result<(), String> { ai.with_store(|st| st.delete_permanently(&id))?; let _ = app.emit("conversations-changed", ()); Ok(()) }

// ---------------------------------------------------------------- chat
#[derive(Serialize)] pub struct Sent { conversation: Conversation, user_message: Message, assistant_message: Message }
#[derive(Serialize, Clone)] struct Token { conversation: String, message: String, delta: String }
#[derive(Serialize, Clone)] struct Done { conversation: String, message: String, perf: Perf, cancelled: bool, summarised: bool }
#[derive(Serialize, Clone)] struct ChatErr { conversation: String, message: String, error: String }
const SYSTEM_PROMPT: &str = "You are Veyra, a private assistant running entirely on the user's own computer. Answer in the language the user writes in. Be accurate and concise; use Markdown, and fenced code blocks with a language tag for code. If you do not know something or cannot do it, say so plainly instead of guessing.";
fn hash(s: &str) -> u64 { use std::hash::{Hash, Hasher}; let mut h = std::collections::hash_map::DefaultHasher::new(); s.hash(&mut h); h.finish() }

/// Sends a user message and starts generating the reply in the background. Returns immediately with the stored messages; text arrives as events.
#[tauri::command] pub async fn chat_send(app: AppHandle, ai: A<'_>, conversation: Option<String>, text: String) -> Result<Sent, String> {
    let text = text.trim().to_string(); if text.is_empty() { return Err("empty message".into()); }
    if ai.loading.load(Ordering::Relaxed) { return Err("The model is still loading. It will be ready in a moment.".into()); }
    if lk(&ai.loaded).is_none() { return Err("No model is loaded. Open the application → Artificial intelligence to download and load one.".into()); }
    let cancel = Arc::new(AtomicBool::new(false));
    { let mut c = lk(&ai.gen_cancel); if c.is_some() { return Err("A reply is already being generated.".into()); } *c = Some(cancel.clone()); }
    let prep = ai.with_store(|st| {
        let conv = match &conversation { Some(id) => st.conversation(id)?, None => st.create_conversation("New conversation", now_s())? };
        let first = st.messages(&conv.id)?.is_empty(); let um = st.add_message(&conv.id, "user", &text, "sent", now_s())?;
        if first { let t: String = text.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(48).collect(); st.rename(&conv.id, &t)?; }
        let am = st.add_message(&conv.id, "assistant", "", "pending", now_s())?; Ok((st.conversation(&conv.id)?, um, am))
    });
    let (conv, um, am) = match prep { Ok(x) => x, Err(e) => { *lk(&ai.gen_cancel) = None; return Err(e); } };
    let a = ai.inner().clone(); let (cid, mid) = (conv.id.clone(), am.id.clone()); let thinking = crate::settings(&app).ai.thinking == "on";
    emit_state(&app, &a); let _ = app.emit("conversations-changed", ());
    spawn_blocking(move || {
        let r = generate(&app, &a, &cid, &mid, thinking, &cancel);
        if let Err(e) = r { let _ = app.emit("chat-error", ChatErr { conversation: cid.clone(), message: mid.clone(), error: e }); }
        *lk(&a.gen_cancel) = None; a.touch(); emit_state(&app, &a); let _ = app.emit("conversations-changed", ());
    });
    Ok(Sent { conversation: conv, user_message: um, assistant_message: am })
}

fn generate(app: &AppHandle, a: &Arc<Ai>, cid: &str, mid: &str, thinking: bool, cancel: &Arc<AtomicBool>) -> Result<(), String> {
    let mut g = lk(&a.engine); let eng = g.as_mut().ok_or("The model was unloaded.")?;
    let all: Vec<Message> = a.with_store(|st| st.messages(cid))?; let ai_set = crate::settings(app).ai;
    let history: Vec<Msg> = all.iter().filter(|m| m.id != mid && !(m.role == "assistant" && m.status == "error" && m.content.is_empty())).map(|m| Msg { role: m.role.clone(), content: m.content.clone() }).collect();
    let ctx = eng.n_ctx() as usize; let reply = (ai_set.max_reply_tokens as usize).min(ctx / 3); let budget = ctx.saturating_sub(reply + 64);
    // exact token counts from the loaded model's own tokenizer, cached by content
    let mut counts: HashMap<String, usize> = HashMap::new();
    let stored = a.with_store(|st| Ok(st.conversation(cid)?.summary))?; let (covers, summary_text) = context::decode_summary(stored.as_deref());
    let mut need: Vec<&str> = history.iter().map(|m| m.content.as_str()).collect(); need.push(SYSTEM_PROMPT); if let Some(t) = &summary_text { need.push(t); }
    for t in need { if counts.contains_key(t) { continue; } let h = hash(t); let cached = lk(&a.tok_cache).get(&h).copied(); let n = match cached { Some(n) => n, None => { let n = eng.count_tokens(t); lk(&a.tok_cache).insert(h, n); n } }; counts.insert(t.to_string(), n); }
    let count = |t: &str| counts.get(t).copied().unwrap_or_else(|| t.len() / 3 + 1);
    let mut built = context::build(SYSTEM_PROMPT, summary_text.as_deref(), &history, budget, &count); let mut summarised = false;
    if built.first_included > covers {
        // turns older than the verbatim window are not covered by the stored summary yet: extend it (old summary + newly dropped turns), then rebuild
        let newly = &history[covers.min(built.first_included)..built.first_included]; let req = context::summary_request(summary_text.as_deref(), newly);
        let (txt, _) = eng.chat(&req, &GenParams { max_tokens: 400, temperature: 0.2, top_p: 0.9, top_k: 20, thinking: false }, cancel, |_| {}, |_| {}).map_err(|e| format!("Summarising the older messages failed: {e}"))?;
        let enc = context::encode_summary(built.first_included, &txt); a.with_store(|st| st.set_summary(cid, &enc))?; summarised = true;
        counts.insert(txt.clone(), eng.count_tokens(&txt)); let count2 = |t: &str| counts.get(t).copied().unwrap_or_else(|| t.len() / 3 + 1);
        built = context::build(SYSTEM_PROMPT, Some(&txt), &history, budget, &count2);
    }
    // live streaming: each piece goes to every window the moment the engine produces it; the partial answer is saved every 2 s so a crash loses little
    let mut acc = String::new(); let mut last_save = Instant::now(); let (appc, cc, mc, ac) = (app.clone(), cid.to_string(), mid.to_string(), a.clone());
    let p = GenParams { max_tokens: ai_set.max_reply_tokens, thinking, ..GenParams::for_mode(thinking) };
    let res = eng.chat(&built.messages, &p, cancel, |d| {
        acc.push_str(d); let _ = appc.emit("chat-token", Token { conversation: cc.clone(), message: mc.clone(), delta: d.to_string() });
        if last_save.elapsed() > Duration::from_secs(2) { let _ = ac.with_store(|st| st.set_message(&mc, &acc, "pending")); last_save = Instant::now(); }
    }, |_| {});
    match res {
        Ok((text, perf)) => { a.with_store(|st| st.set_message(mid, &text, "sent"))?; *lk(&a.last_perf) = Some(perf.clone()); *lk(&a.base_status) = Some(eng.status());
            let _ = app.emit("chat-done", Done { conversation: cid.into(), message: mid.into(), perf, cancelled: false, summarised }); Ok(()) }
        Err(EngineError::Cancelled) => { a.with_store(|st| st.set_message(mid, &acc, "sent"))?; let _ = app.emit("chat-done", Done { conversation: cid.into(), message: mid.into(), perf: Perf::default(), cancelled: true, summarised }); Ok(()) }
        Err(e) => { let _ = a.with_store(|st| st.set_message(mid, &acc, "error")); Err(friendly(&e)) }
    }
}

/// Called on exit: stop generation and free the model.
pub fn shutdown(ai: &Arc<Ai>) {
    if let Some(c) = lk(&ai.gen_cancel).as_ref() { c.store(true, Ordering::Relaxed); } if let Some(c) = lk(&ai.dl_cancel).as_ref() { c.store(true, Ordering::Relaxed); }
    let mut g = lk(&ai.engine); if let Some(e) = g.take() { e.unload(); }
}
