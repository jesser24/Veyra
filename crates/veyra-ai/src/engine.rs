//! The inference engine, running INSIDE the Veyra process (libllama linked statically through `veyra-llama`): no child process, no HTTP server, no port.
//! One model at a time. Loading and generation are blocking calls: the application runs them on worker threads and streams events to the UI.
//! Unload = dropping the engine, which frees the model and the KV cache in this process.
use crate::context::Msg;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::Instant;
use veyra_llama::{Outcome, Sampling, Utf8Stream};

#[derive(Clone, Debug)]
pub struct EngineConfig { pub model: PathBuf, pub ctx: u32, pub threads: usize, pub batch: u32, pub ubatch: u32, pub flash_attn: i32, pub kv_q8: bool, pub gpu_layers: i32 }
impl EngineConfig { pub fn new(model: PathBuf, ctx: u32, threads: usize) -> Self { EngineConfig { model, ctx, threads, batch: 512, ubatch: 512, flash_attn: -1, kv_q8: false, gpu_layers: 0 } } }

#[derive(Debug)]
pub enum EngineError { Load(String), Generate(String), Cancelled }
impl std::fmt::Display for EngineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self { EngineError::Load(e) => write!(f, "cannot load the model: {e}"), EngineError::Generate(e) => write!(f, "generation failed: {e}"), EngineError::Cancelled => write!(f, "generation cancelled") }
    }
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Perf {
    pub prompt_tokens: Option<u64>, pub prompt_cached: Option<u64>, pub generated_tokens: Option<u64>,
    pub prompt_tokens_per_s: Option<f64>, pub generated_tokens_per_s: Option<f64>,
    pub prefill_ms: Option<u64>, pub decode_ms: Option<u64>, pub first_token_ms: Option<u64>, pub wall_ms: u64, pub load_ms: Option<u64>,
}
#[derive(Clone, Debug, Serialize)]
pub struct Status { pub loaded: bool, pub model: Option<String>, pub ctx: u32, pub threads: usize, pub rss_mib: Option<u64>, pub load_ms: Option<u64>, pub in_process: bool, pub llama: String }

#[derive(Clone, Debug)]
pub struct GenParams { pub max_tokens: u32, pub temperature: f32, pub top_p: f32, pub top_k: i32, pub thinking: bool }
impl Default for GenParams { fn default() -> Self { Self { max_tokens: 1024, temperature: 0.7, top_p: 0.8, top_k: 20, thinking: false } } }
impl GenParams { /// Qwen3's recommended sampling: 0.7/0.8 without thinking, 0.6/0.95 with it (greedy decoding is discouraged in thinking mode).
    pub fn for_mode(thinking: bool) -> Self { if thinking { Self { temperature: 0.6, top_p: 0.95, thinking, max_tokens: 2048, ..Self::default() } } else { Self::default() } } }

pub struct Engine { inner: veyra_llama::Engine, cfg: EngineConfig, load_ms: u64, qwen3_thinking_switch: bool }

pub fn own_rss_mib() -> Option<u64> { use sysinfo::{Pid, ProcessesToUpdate, System}; let mut s = System::new(); let p = Pid::from_u32(std::process::id()); s.refresh_processes(ProcessesToUpdate::Some(&[p]), true); s.process(p).map(|p| p.memory() / 1_048_576) }

impl Engine {
    /// Blocking (reads and maps the model file): call from a worker thread.
    pub fn load(cfg: EngineConfig) -> Result<Engine, EngineError> {
        let t = Instant::now();
        let c = veyra_llama::Config { n_ctx: cfg.ctx, n_batch: cfg.batch, n_ubatch: cfg.ubatch, n_threads: cfg.threads as u32, n_threads_batch: cfg.threads as u32, flash_attn: cfg.flash_attn, kv_q8: cfg.kv_q8, n_gpu_layers: cfg.gpu_layers, use_mmap: true };
        let inner = veyra_llama::Engine::load(&cfg.model, &c).map_err(|e| EngineError::Load(e.0))?;
        let tpl = inner.model_template(); let switch = tpl.contains("enable_thinking");
        Ok(Engine { inner, cfg, load_ms: t.elapsed().as_millis() as u64, qwen3_thinking_switch: switch })
    }
    pub fn status(&self) -> Status {
        Status { loaded: true, model: self.cfg.model.file_name().map(|n| n.to_string_lossy().into_owned()), ctx: self.inner.n_ctx, threads: self.cfg.threads, rss_mib: own_rss_mib(), load_ms: Some(self.load_ms), in_process: true, llama: veyra_llama::version() }
    }
    pub fn n_ctx(&self) -> u32 { self.inner.n_ctx }
    pub fn model_bytes(&self) -> u64 { self.inner.model_bytes }
    pub fn count_tokens(&self, text: &str) -> usize { self.inner.count_tokens(text) }

    /// Prompt text for the model: its own chat template when llama.cpp knows it, ChatML otherwise. Qwen3-style templates get the empty think block
    /// that turns thinking off (that is what the official template's `enable_thinking=false` does).
    pub fn format_prompt(&self, msgs: &[Msg], thinking: bool) -> String {
        let pairs: Vec<(&str, &str)> = msgs.iter().map(|m| (m.role.as_str(), m.content.as_str())).collect();
        let mut p = self.inner.apply_template(&pairs, true).unwrap_or_else(|| chatml(msgs));
        if self.qwen3_thinking_switch && !thinking { p.push_str("<think>\n\n</think>\n\n"); }
        p
    }

    /// Blocking generation. Answer text goes to `on_delta`, reasoning text (between <think> tags) to `on_reasoning`.
    pub fn chat(&mut self, messages: &[Msg], p: &GenParams, cancel: &AtomicBool, mut on_delta: impl FnMut(&str), mut on_reasoning: impl FnMut(&str)) -> Result<(String, Perf), EngineError> {
        let prompt = self.format_prompt(messages, p.thinking);
        let s = Sampling { max_tokens: p.max_tokens, temperature: p.temperature, top_p: p.top_p, top_k: p.top_k, ..Sampling::default() };
        let (mut utf8, mut split, mut answer) = (Utf8Stream::default(), ThinkSplitter::default(), String::new()); let t0 = Instant::now();
        let r = self.inner.generate(&prompt, &s, cancel, |bytes| {
            let text = utf8.push(bytes); if text.is_empty() { return false; }
            for seg in split.feed(&text) { match seg { Seg::Answer(t) => { answer.push_str(&t); on_delta(&t); } Seg::Reasoning(t) => on_reasoning(&t) } }
            false
        });
        match r {
            Ok((outcome, st)) => {
                for seg in split.finish() { match seg { Seg::Answer(t) => { answer.push_str(&t); on_delta(&t); } Seg::Reasoning(t) => on_reasoning(&t) } }
                if outcome == Outcome::Cancelled { return Err(EngineError::Cancelled); }
                let new_prompt = st.prompt_tokens.saturating_sub(st.prompt_cached) as f64;
                Ok((answer, Perf { prompt_tokens: Some(st.prompt_tokens as u64), prompt_cached: Some(st.prompt_cached as u64), generated_tokens: Some(st.generated_tokens as u64),
                    prompt_tokens_per_s: (st.prefill_ms > 0.0 && new_prompt > 0.0).then(|| new_prompt / (st.prefill_ms / 1000.0)), generated_tokens_per_s: (st.decode_ms > 0.0 && st.generated_tokens > 0).then(|| st.generated_tokens as f64 / (st.decode_ms / 1000.0)),
                    prefill_ms: Some(st.prefill_ms as u64), decode_ms: Some(st.decode_ms as u64), first_token_ms: Some(st.first_token_ms as u64), wall_ms: t0.elapsed().as_millis() as u64, load_ms: Some(self.load_ms) }))
            }
            Err(e) => Err(EngineError::Generate(e.0)),
        }
    }
    /// Frees the model and context now (the memory returns to the OS).
    pub fn unload(self) { drop(self) }
}

fn chatml(msgs: &[Msg]) -> String { let mut s = String::new(); for m in msgs { s.push_str(&format!("<|im_start|>{}\n{}<|im_end|>\n", m.role, m.content)); } s.push_str("<|im_start|>assistant\n"); s }

#[derive(Debug, PartialEq, Eq)] pub enum Seg { Answer(String), Reasoning(String) }

/// Streaming splitter for `<think>…</think>`: tags may arrive split across pieces, so a possible partial tag is held back until it is resolved.
#[derive(Default)]
pub struct ThinkSplitter { buf: String, inside: bool }
impl ThinkSplitter {
    pub fn feed(&mut self, text: &str) -> Vec<Seg> { self.buf.push_str(text); self.drain(false) }
    pub fn finish(&mut self) -> Vec<Seg> { self.drain(true) }
    fn drain(&mut self, last: bool) -> Vec<Seg> {
        let mut out = vec![];
        loop {
            let tag = if self.inside { "</think>" } else { "<think>" };
            if let Some(i) = self.buf.find(tag) {
                let before: String = self.buf.drain(..i).collect(); self.buf.drain(..tag.len()); if !before.is_empty() { out.push(self.seg(before)); } self.inside = !self.inside;
                if !self.inside { let t = self.buf.trim_start_matches('\n').len(); let cut = self.buf.len() - t; self.buf.drain(..cut); } // blank lines after </think> are formatting, not answer
                continue;
            }
            // hold back the longest suffix that could still become the tag
            let keep = if last { 0 } else { (1..tag.len()).rev().find(|n| self.buf.len() >= *n && self.buf.is_char_boundary(self.buf.len() - n) && tag.starts_with(&self.buf[self.buf.len() - n..])).unwrap_or(0) };
            let emit: String = self.buf.drain(..self.buf.len() - keep).collect(); if !emit.is_empty() { out.push(self.seg(emit)); } break;
        }
        out
    }
    fn seg(&self, t: String) -> Seg { if self.inside { Seg::Reasoning(t) } else { Seg::Answer(t) } }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(parts: &[&str]) -> (String, String) { let mut s = ThinkSplitter::default(); let (mut a, mut r) = (String::new(), String::new());
        let mut take = |v: Vec<Seg>| for x in v { match x { Seg::Answer(t) => a.push_str(&t), Seg::Reasoning(t) => r.push_str(&t) } };
        for p in parts { take(s.feed(p)); } take(s.finish()); (a, r) }
    #[test] fn plain_answers_pass_through_untouched() { assert_eq!(run(&["Hello ", "world"]), ("Hello world".into(), "".into())); }
    #[test] fn reasoning_is_separated_even_when_tags_are_split_across_pieces() {
        assert_eq!(run(&["<thi", "nk>why", " so</th", "ink>\n\nAnswer"]), ("Answer".into(), "why so".into()));
        assert_eq!(run(&["a<", "b"]), ("a<b".into(), "".into()), "a lone '<' that is not a tag is released");
    }
    #[test] fn an_unterminated_think_block_stays_reasoning_and_nothing_is_lost() { assert_eq!(run(&["<think>still thinking"]), ("".into(), "still thinking".into())); }
    #[test] fn text_ending_with_partial_tag_is_flushed_on_finish() { assert_eq!(run(&["x <thi"]), ("x <thi".into(), "".into())); }
    #[test] fn multibyte_text_is_never_cut() { assert_eq!(run(&["é<th", "é"]), ("é<thé".into(), "".into())); }
    #[test] fn chatml_fallback_is_well_formed() { let s = chatml(&[Msg { role: "user".into(), content: "hi".into() }]); assert_eq!(s, "<|im_start|>user\nhi<|im_end|>\n<|im_start|>assistant\n"); }
}
