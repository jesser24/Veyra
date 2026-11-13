//! Model catalog and recommendation. Entries marked `Verified` carry the SHA-256 published by the model's official Hugging Face page
//! (read on 2026-11-12) and are re-checked after every download. `Candidate` entries are NOT offered until benchmarked and hash-verified.
use crate::hardware::Hardware;
use serde::Serialize;

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)] #[serde(rename_all = "snake_case")]
pub enum Status { Verified, Candidate }

#[derive(Serialize, Clone, Debug)]
pub struct ModelSpec {
    pub id: &'static str, pub name: &'static str, pub file: &'static str, pub url: &'static str, pub sha256: Option<&'static str>,
    pub size_gb: f64, pub params_b: f64, pub quant: &'static str, pub license: &'static str, pub official_gguf: bool, pub status: Status,
    /// Rough working set: file + KV cache + runtime. The real figure is measured after loading.
    pub min_ram_gb: f64, pub notes: &'static str,
}

pub const CATALOG: &[ModelSpec] = &[
    ModelSpec { id: "qwen3-4b-q5_k_m", name: "Qwen3 4B (Q5_K_M)", file: "Qwen3-4B-Q5_K_M.gguf", url: "https://huggingface.co/Qwen/Qwen3-4B-GGUF/resolve/main/Qwen3-4B-Q5_K_M.gguf",
        sha256: Some("aca596860e8cb40af6539e3f2ea40df305f42515deac56d49c08d39a02e6533f"), size_gb: 2.89, params_b: 4.0, quant: "Q5_K_M", license: "Apache-2.0", official_gguf: true, status: Status::Verified, min_ram_gb: 6.0,
        notes: "Default. Official GGUF, thinking and non-thinking modes, 32K native context, multilingual." },
    ModelSpec { id: "qwen3-4b-q4_k_m", name: "Qwen3 4B (Q4_K_M)", file: "Qwen3-4B-Q4_K_M.gguf", url: "https://huggingface.co/Qwen/Qwen3-4B-GGUF/resolve/main/Qwen3-4B-Q4_K_M.gguf",
        sha256: Some("7485fe6f11af29433bc51cab58009521f205840f5b4ae3a32fa7f92e8534fdf5"), size_gb: 2.5, params_b: 4.0, quant: "Q4_K_M", license: "Apache-2.0", official_gguf: true, status: Status::Verified, min_ram_gb: 5.0,
        notes: "Smaller and faster than Q5_K_M with a small quality loss; for machines with little RAM." },
    ModelSpec { id: "qwen3-8b-q4_k_m", name: "Qwen3 8B (Q4_K_M)", file: "Qwen3-8B-Q4_K_M.gguf", url: "https://huggingface.co/Qwen/Qwen3-8B-GGUF/resolve/main/Qwen3-8B-Q4_K_M.gguf",
        sha256: Some("d98cdcbd03e17ce47681435b5150e34c1417f50b5c0019dd560e4882c5745785"), size_gb: 5.03, params_b: 8.0, quant: "Q4_K_M", license: "Apache-2.0", official_gguf: true, status: Status::Verified, min_ram_gb: 9.0,
        notes: "Better reasoning and code than 4B but about twice as slow on CPU; only worth it with 16 GB+ RAM and patience." },
    ModelSpec { id: "qwen3-0.6b-q8_0", name: "Qwen3 0.6B (Q8_0) — test model", file: "Qwen3-0.6B-Q8_0.gguf", url: "https://huggingface.co/Qwen/Qwen3-0.6B-GGUF/resolve/main/Qwen3-0.6B-Q8_0.gguf",
        sha256: Some("9465e63a22add5354d9bb4b99e90117043c7124007664907259bd16d043bb031"), size_gb: 0.64, params_b: 0.6, quant: "Q8_0", license: "Apache-2.0", official_gguf: true, status: Status::Verified, min_ram_gb: 2.0,
        notes: "Tiny; used to test the engine end to end. Not recommended for real use." },
    ModelSpec { id: "qwen3.5-4b", name: "Qwen3.5 4B (candidate)", file: "", url: "", sha256: None, size_gb: 0.0, params_b: 4.0, quant: "?", license: "Apache-2.0", official_gguf: false, status: Status::Candidate, min_ram_gb: 6.0,
        notes: "Released Feb 2026: hybrid Gated-DeltaNet architecture, 262K context, 201 languages, vision. No official GGUF found; needs llama.cpp support check, a verified community GGUF hash and a benchmark on the reference Dell before it can replace Qwen3 4B." },
];

#[derive(Serialize, Clone, Debug)]
pub struct Recommendation { pub model: &'static ModelSpec, pub reason: String, pub warnings: Vec<String>, pub alternatives: Vec<&'static ModelSpec> }

/// Recommend among VERIFIED models only. Rule: the largest quality tier that leaves comfortable headroom (model + 2 GB for the OS and KV cache
/// must fit in *total* RAM, model must fit in *available* RAM), preferring the 4B Q5_K_M default; the 8B is offered as an alternative, never the default on CPU.
pub fn recommend(h: &Hardware) -> Option<Recommendation> {
    let total = h.total_ram_mib as f64 / 1024.0; let avail = h.available_ram_mib as f64 / 1024.0;
    let fits = |m: &ModelSpec| total >= m.min_ram_gb + 2.0 && avail >= m.size_gb + 0.8;
    let verified: Vec<&'static ModelSpec> = CATALOG.iter().filter(|m| m.status == Status::Verified && m.params_b >= 1.0).collect();
    let pick = ["qwen3-4b-q5_k_m", "qwen3-4b-q4_k_m"].iter().filter_map(|id| verified.iter().copied().find(|m| m.id == *id)).find(|m| fits(m))?;
    let mut warnings = vec![];
    if !h.avx2 && matches!(h.arch.as_str(), "x86_64" | "x86") { warnings.push("This CPU lacks AVX2: generation will be very slow.".into()); }
    if h.physical_cores < 4 { warnings.push(format!("Only {} physical CPU core(s): expect slow generation.", h.physical_cores)); }
    let alternatives = verified.iter().copied().filter(|m| m.id != pick.id && fits(m)).collect();
    Some(Recommendation { model: pick, reason: format!("{} fits your {:.0} GB of RAM ({:.1} GB free) with headroom, is officially published as GGUF under {}, and runs on CPU. Speed on your machine is measured after the first load, not promised.", pick.name, total, avail, pick.license), warnings, alternatives })
}
pub fn by_id(id: &str) -> Option<&'static ModelSpec> { CATALOG.iter().find(|m| m.id == id) }

#[cfg(test)]
mod tests {
    use super::*;
    fn hw(total_gb: u64, avail_gb: u64, cores: usize, avx2: bool) -> Hardware { Hardware { os: "t".into(), arch: "x86_64".into(), cpu_brand: "t".into(), physical_cores: cores, logical_cores: cores * 2, total_ram_mib: total_gb * 1024, available_ram_mib: avail_gb * 1024, avx2, avx512: false, apple_silicon: false } }
    #[test] fn reference_dell_gets_qwen3_4b_q5_with_the_8b_as_an_option() {
        let r = recommend(&hw(24, 18, 4, true)).unwrap(); assert_eq!(r.model.id, "qwen3-4b-q5_k_m"); assert!(r.alternatives.iter().any(|m| m.id == "qwen3-8b-q4_k_m")); assert!(r.warnings.is_empty());
    }
    #[test] fn small_machines_get_the_smaller_quantisation_or_nothing() {
        assert_eq!(recommend(&hw(7, 4, 4, true)).unwrap().model.id, "qwen3-4b-q4_k_m");
        assert!(recommend(&hw(4, 2, 4, true)).is_none(), "never recommend a model that does not fit");
    }
    #[test] fn warnings_for_weak_cpus() { let r = recommend(&hw(16, 10, 2, false)).unwrap(); assert_eq!(r.warnings.len(), 2); }
    #[test] fn candidates_and_test_models_are_never_recommended_and_every_verified_entry_has_a_hash_and_url() {
        for m in CATALOG { if m.status == Status::Verified { assert_eq!(m.sha256.unwrap().len(), 64, "{}", m.id); assert!(m.url.starts_with("https://huggingface.co/Qwen/") && m.file.ends_with(".gguf")); } else { assert!(m.sha256.is_none() && m.url.is_empty()); } }
        for t in [(64, 60, 8, true), (24, 18, 4, true), (8, 4, 4, true)] { let r = recommend(&hw(t.0, t.1, t.2, t.3)).unwrap(); assert!(r.model.params_b >= 1.0 && r.model.status == Status::Verified); }
    }
}
