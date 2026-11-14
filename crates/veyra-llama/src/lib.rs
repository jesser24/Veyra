//! Safe wrapper over the Veyra libllama shim. One `Engine` = one model + one context, living inside the calling process.
//! Not `Sync`: generation takes `&mut self`; the application keeps the engine behind a mutex and runs calls on a worker thread.
use std::ffi::{c_char, c_int, c_void, CString};
use std::path::Path;
use std::sync::atomic::AtomicBool;

#[derive(Clone, Debug)]
pub struct Config { pub n_ctx: u32, pub n_batch: u32, pub n_ubatch: u32, pub n_threads: u32, pub n_threads_batch: u32, pub flash_attn: i32, pub kv_q8: bool, pub n_gpu_layers: i32, pub use_mmap: bool }
impl Default for Config { fn default() -> Self { Config { n_ctx: 4096, n_batch: 512, n_ubatch: 512, n_threads: 4, n_threads_batch: 4, flash_attn: -1, kv_q8: false, n_gpu_layers: 0, use_mmap: true } } }

#[derive(Clone, Debug)]
pub struct Sampling { pub max_tokens: u32, pub temperature: f32, pub top_p: f32, pub min_p: f32, pub top_k: i32, pub seed: u32 }
impl Default for Sampling { fn default() -> Self { Sampling { max_tokens: 1024, temperature: 0.7, top_p: 0.8, min_p: 0.0, top_k: 20, seed: 0xA11CE } } }

#[derive(Clone, Debug, Default)]
pub struct Stats { pub prompt_tokens: u32, pub prompt_cached: u32, pub generated_tokens: u32, pub prefill_ms: f64, pub decode_ms: f64, pub first_token_ms: f64 }
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome { Finished, Cancelled }
#[derive(Debug)]
pub struct Error(pub String);
impl std::fmt::Display for Error { fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str(&self.0) } }
impl std::error::Error for Error {}

#[cfg(not(veyra_stub))]
mod ffi {
    use super::*;
    #[repr(C)] pub struct RawCfg { pub model_path: *const c_char, pub n_ctx: i32, pub n_batch: i32, pub n_ubatch: i32, pub n_threads: i32, pub n_threads_batch: i32, pub flash_attn: i32, pub kv_q8: i32, pub n_gpu_layers: i32, pub use_mmap: i32 }
    #[repr(C)] pub struct RawGen { pub max_tokens: i32, pub temperature: f32, pub top_p: f32, pub min_p: f32, pub top_k: i32, pub seed: u32 }
    #[repr(C)] #[derive(Default)] pub struct RawStats { pub prompt_tokens: i32, pub prompt_cached: i32, pub generated_tokens: i32, pub prefill_ms: f64, pub decode_ms: f64, pub first_token_ms: f64 }
    pub type PieceCb = unsafe extern "C" fn(*const c_char, i32, *mut c_void) -> c_int;
    pub enum Raw {}
    extern "C" {
        pub fn veyra_llama_version() -> *const c_char;
        pub fn veyra_engine_new(cfg: *const RawCfg, err: *mut c_char, errlen: usize) -> *mut Raw;
        pub fn veyra_engine_free(e: *mut Raw);
        pub fn veyra_n_ctx(e: *const Raw) -> i32; pub fn veyra_n_ctx_train(e: *const Raw) -> i32; pub fn veyra_model_bytes(e: *const Raw) -> u64;
        pub fn veyra_model_template(e: *const Raw, buf: *mut c_char, cap: usize) -> i32;
        pub fn veyra_tokenize(e: *const Raw, text: *const c_char, len: i32, special: i32, out: *mut i32, cap: i32) -> i32;
        pub fn veyra_apply_template(e: *const Raw, roles: *const *const c_char, contents: *const *const c_char, n: usize, add: i32, out: *mut c_char, cap: i32) -> i32;
        pub fn veyra_generate(e: *mut Raw, prompt: *const c_char, len: i32, g: *const RawGen, cb: PieceCb, user: *mut c_void, cancel: *const u8, st: *mut RawStats, err: *mut c_char, errlen: usize) -> i32;
        pub fn veyra_reset(e: *mut Raw);
    }
}

/// llama.cpp version this binary was built against ("stub" when built without the native libraries).
pub fn version() -> String {
    #[cfg(not(veyra_stub))] { unsafe { std::ffi::CStr::from_ptr(ffi::veyra_llama_version()) }.to_string_lossy().into_owned() }
    #[cfg(veyra_stub)] { "stub (built without llama.cpp)".into() }
}
pub const IS_STUB: bool = cfg!(veyra_stub);

#[cfg(not(veyra_stub))]
pub struct Engine { raw: *mut ffi::Raw, pub model_bytes: u64, pub n_ctx: u32, pub n_ctx_train: u32 }
#[cfg(not(veyra_stub))]
unsafe impl Send for Engine {}
#[cfg(veyra_stub)]
pub struct Engine { pub model_bytes: u64, pub n_ctx: u32, pub n_ctx_train: u32 }

#[cfg(not(veyra_stub))]
impl Engine {
    pub fn load(model: &Path, c: &Config) -> Result<Engine, Error> {
        let path = CString::new(model.to_string_lossy().as_bytes()).map_err(|_| Error("invalid model path".into()))?;
        let raw = ffi::RawCfg { model_path: path.as_ptr(), n_ctx: c.n_ctx as i32, n_batch: c.n_batch as i32, n_ubatch: c.n_ubatch as i32, n_threads: c.n_threads as i32, n_threads_batch: c.n_threads_batch as i32, flash_attn: c.flash_attn, kv_q8: c.kv_q8 as i32, n_gpu_layers: c.n_gpu_layers, use_mmap: c.use_mmap as i32 };
        let mut err = vec![0 as c_char; 2048];
        let e = unsafe { ffi::veyra_engine_new(&raw, err.as_mut_ptr(), err.len()) };
        if e.is_null() { return Err(Error(cstr(&err))); }
        Ok(Engine { raw: e, model_bytes: unsafe { ffi::veyra_model_bytes(e) }, n_ctx: unsafe { ffi::veyra_n_ctx(e) } as u32, n_ctx_train: unsafe { ffi::veyra_n_ctx_train(e) } as u32 })
    }
    pub fn model_template(&self) -> String { let mut b = vec![0 as c_char; 32768]; unsafe { ffi::veyra_model_template(self.raw, b.as_mut_ptr(), b.len()) }; cstr(&b) }
    pub fn count_tokens(&self, text: &str) -> usize {
        let mut cap = text.len() as i32 + 16; loop {
            let mut out = vec![0i32; cap as usize]; let n = unsafe { ffi::veyra_tokenize(self.raw, text.as_ptr() as *const c_char, text.len() as i32, 0, out.as_mut_ptr(), cap) };
            if n >= 0 { return n as usize; } if n == i32::MIN { return text.len() / 3 + 1; } cap = -n + 8;
        }
    }
    /// Formats messages with the model's own chat template (None if llama.cpp's built-in formatter does not know it).
    pub fn apply_template(&self, msgs: &[(&str, &str)], add_assistant: bool) -> Option<String> {
        let roles: Vec<CString> = msgs.iter().map(|m| CString::new(m.0).unwrap_or_default()).collect(); let contents: Vec<CString> = msgs.iter().map(|m| CString::new(m.1.replace('\0', "")).unwrap_or_default()).collect();
        let rp: Vec<*const c_char> = roles.iter().map(|c| c.as_ptr()).collect(); let cp: Vec<*const c_char> = contents.iter().map(|c| c.as_ptr()).collect();
        let mut cap = msgs.iter().map(|m| m.1.len() + m.0.len() + 32).sum::<usize>() as i32 + 256;
        loop {
            let mut out = vec![0u8; cap as usize]; let n = unsafe { ffi::veyra_apply_template(self.raw, rp.as_ptr(), cp.as_ptr(), msgs.len(), add_assistant as i32, out.as_mut_ptr() as *mut c_char, cap) };
            if n == i32::MIN { return None; } if n < 0 { cap = -n + 8; continue; } out.truncate(n as usize); return String::from_utf8(out).ok();
        }
    }
    /// Blocking generation. `on_piece` receives raw bytes (a multi-byte character may be split across pieces) and returns true to stop.
    pub fn generate(&mut self, prompt: &str, s: &Sampling, cancel: &AtomicBool, mut on_piece: impl FnMut(&[u8]) -> bool) -> Result<(Outcome, Stats), Error> {
        unsafe extern "C" fn tramp(p: *const c_char, len: i32, user: *mut c_void) -> c_int {
            let f = &mut *(user as *mut &mut dyn FnMut(&[u8]) -> bool);
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(std::slice::from_raw_parts(p as *const u8, len as usize)))) { Ok(stop) => stop as c_int, Err(_) => 1 }
        }
        let g = ffi::RawGen { max_tokens: s.max_tokens as i32, temperature: s.temperature, top_p: s.top_p, min_p: s.min_p, top_k: s.top_k, seed: s.seed };
        let mut st = ffi::RawStats::default(); let mut err = vec![0 as c_char; 2048];
        let mut dynf: &mut dyn FnMut(&[u8]) -> bool = &mut on_piece;
        let rc = unsafe { ffi::veyra_generate(self.raw, prompt.as_ptr() as *const c_char, prompt.len() as i32, &g, tramp, &mut dynf as *mut _ as *mut c_void, cancel as *const AtomicBool as *const u8, &mut st, err.as_mut_ptr(), err.len()) };
        let stats = Stats { prompt_tokens: st.prompt_tokens as u32, prompt_cached: st.prompt_cached as u32, generated_tokens: st.generated_tokens as u32, prefill_ms: st.prefill_ms, decode_ms: st.decode_ms, first_token_ms: st.first_token_ms };
        match rc { 0 => Ok((Outcome::Finished, stats)), 1 => Ok((Outcome::Cancelled, stats)), _ => Err(Error(cstr(&err))) }
    }
    pub fn reset(&mut self) { unsafe { ffi::veyra_reset(self.raw) } }
}
#[cfg(not(veyra_stub))]
impl Drop for Engine { fn drop(&mut self) { unsafe { ffi::veyra_engine_free(self.raw) } } } // frees the model and its memory in this process

#[cfg(veyra_stub)]
impl Engine {
    pub fn load(_: &Path, _: &Config) -> Result<Engine, Error> { Err(Error("this build of Veyra does not include the inference library (built without VEYRA_LLAMA_DIR)".into())) }
    pub fn model_template(&self) -> String { String::new() }
    pub fn count_tokens(&self, t: &str) -> usize { t.len() / 3 + 1 }
    pub fn apply_template(&self, _: &[(&str, &str)], _: bool) -> Option<String> { None }
    pub fn generate(&mut self, _: &str, _: &Sampling, _: &AtomicBool, _: impl FnMut(&[u8]) -> bool) -> Result<(Outcome, Stats), Error> { Err(Error("stub".into())) }
    pub fn reset(&mut self) {}
}

#[cfg(not(veyra_stub))]
fn cstr(b: &[c_char]) -> String { let bytes: Vec<u8> = b.iter().take_while(|c| **c != 0).map(|c| *c as u8).collect(); String::from_utf8_lossy(&bytes).into_owned() }

/// Accumulates raw bytes and yields only complete UTF-8 text (a token may end in the middle of a multi-byte character).
#[derive(Default)]
pub struct Utf8Stream { pending: Vec<u8> }
impl Utf8Stream {
    pub fn push(&mut self, bytes: &[u8]) -> String {
        self.pending.extend_from_slice(bytes);
        match std::str::from_utf8(&self.pending) {
            Ok(s) => { let o = s.to_string(); self.pending.clear(); o }
            Err(e) => { let n = e.valid_up_to(); let o = String::from_utf8_lossy(&self.pending[..n]).into_owned();
                if e.error_len().is_some() { let all = String::from_utf8_lossy(&self.pending).into_owned(); self.pending.clear(); all } else { self.pending.drain(..n); o } }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn utf8_stream_waits_for_complete_characters() {
        let mut u = Utf8Stream::default(); let e = "é".as_bytes(); // 2 bytes
        assert_eq!(u.push(&e[..1]), ""); assert_eq!(u.push(&e[1..]), "é"); assert_eq!(u.push(b"abc"), "abc");
        let emoji = "🙂".as_bytes(); assert_eq!(u.push(&emoji[..2]), ""); assert_eq!(u.push(&emoji[2..]), "🙂");
    }
    #[test] fn invalid_bytes_do_not_wedge_the_stream() { let mut u = Utf8Stream::default(); let out = u.push(&[0xff, b'a']); assert!(out.ends_with('a')); assert_eq!(u.push(b"b"), "b"); }
    /// Exercises the real native library (link + shim + llama.cpp's own error path) without needing a model.
    #[cfg(not(veyra_stub))] #[test] fn a_missing_or_corrupt_model_file_is_a_clean_error_not_a_crash() {
        let d = std::env::temp_dir().join(format!("veyra-llama-test-{}", std::process::id())); std::fs::create_dir_all(&d).unwrap();
        let bad = d.join("bad.gguf"); std::fs::write(&bad, b"this is not a gguf file").unwrap();
        for p in [bad.as_path(), d.join("missing.gguf").as_path()] { match Engine::load(p, &Config::default()) { Err(e) => assert!(e.0.contains("cannot load the model"), "{}", e.0), Ok(_) => panic!("must fail") } }
        assert!(version().contains("b12336")); let _ = std::fs::remove_dir_all(d);
    }
    #[cfg(veyra_stub)] #[test] fn stub_reports_a_clear_error() { match Engine::load(Path::new("x"), &Config::default()) { Err(e) => assert!(e.0.contains("does not include")), Ok(_) => panic!("stub must not load") } }
}
