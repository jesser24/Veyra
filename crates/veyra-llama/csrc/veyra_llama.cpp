#include "veyra_llama.h"
#include "llama.h"
#include <algorithm>
#include <chrono>
#include <cstdio>
#include <cstring>
#include <mutex>
#include <string>
#include <vector>

struct veyra_engine {
    llama_model* model = nullptr;
    llama_context* ctx = nullptr;
    const llama_vocab* vocab = nullptr;
    std::vector<llama_token> cache;   // tokens whose KV entries are currently in the context (prompt-prefix reuse between turns)
    int32_t n_batch = 512;
};

static std::mutex g_log_mu;
static std::string g_log_tail;   // last warnings/errors from llama.cpp, used to build useful error messages
static void log_cb(ggml_log_level level, const char* text, void*) {
    if (level != GGML_LOG_LEVEL_ERROR && level != GGML_LOG_LEVEL_WARN) return;   // everything else is dropped: no console noise
    std::lock_guard<std::mutex> g(g_log_mu);
    g_log_tail += text;
    if (g_log_tail.size() > 1500) g_log_tail.erase(0, g_log_tail.size() - 1000);
}
static void set_err(char* err, size_t n, const std::string& m) { if (err && n) { std::snprintf(err, n, "%s", m.c_str()); } }
static std::string tail() { std::lock_guard<std::mutex> g(g_log_mu); return g_log_tail; }
static double ms_since(std::chrono::steady_clock::time_point t) { return std::chrono::duration<double, std::milli>(std::chrono::steady_clock::now() - t).count(); }

const char* veyra_llama_version(void) { return "llama.cpp b12336 (cfb9d4b7)"; }

veyra_engine* veyra_engine_new(const veyra_config* c, char* err, size_t errlen) {
    try {
        static std::once_flag once; std::call_once(once, [] { llama_log_set(log_cb, nullptr); llama_backend_init(); });
        { std::lock_guard<std::mutex> g(g_log_mu); g_log_tail.clear(); }
        llama_model_params mp = llama_model_default_params();
        mp.n_gpu_layers = c->n_gpu_layers; mp.load_mode = c->use_mmap ? LLAMA_LOAD_MODE_MMAP : LLAMA_LOAD_MODE_NONE;
        llama_model* m = llama_model_load_from_file(c->model_path, mp);
        if (!m) { set_err(err, errlen, "cannot load the model file (corrupt, unsupported format or not enough memory). " + tail()); return nullptr; }
        llama_context_params cp = llama_context_default_params();
        cp.n_ctx = (uint32_t)c->n_ctx; cp.n_batch = (uint32_t)c->n_batch; cp.n_ubatch = (uint32_t)c->n_ubatch;
        cp.n_threads = c->n_threads; cp.n_threads_batch = c->n_threads_batch; cp.no_perf = true;
        cp.flash_attn_type = c->flash_attn < 0 ? LLAMA_FLASH_ATTN_TYPE_AUTO : (c->flash_attn ? LLAMA_FLASH_ATTN_TYPE_ENABLED : LLAMA_FLASH_ATTN_TYPE_DISABLED);
        if (c->kv_q8) { cp.type_k = GGML_TYPE_Q8_0; cp.type_v = GGML_TYPE_Q8_0; }
        llama_context* x = llama_init_from_model(m, cp);
        if (!x) { llama_model_free(m); set_err(err, errlen, "cannot create the inference context (not enough memory for this context size?). " + tail()); return nullptr; }
        auto* e = new veyra_engine(); e->model = m; e->ctx = x; e->vocab = llama_model_get_vocab(m); e->n_batch = c->n_batch;
        return e;
    } catch (const std::exception& ex) { set_err(err, errlen, std::string("native error: ") + ex.what()); return nullptr; }
    catch (...) { set_err(err, errlen, "unknown native error"); return nullptr; }
}
void veyra_engine_free(veyra_engine* e) { if (!e) return; if (e->ctx) llama_free(e->ctx); if (e->model) llama_model_free(e->model); delete e; }
int32_t veyra_n_ctx(const veyra_engine* e) { return (int32_t)llama_n_ctx(e->ctx); }
int32_t veyra_n_ctx_train(const veyra_engine* e) { return llama_model_n_ctx_train(e->model); }
uint64_t veyra_model_bytes(const veyra_engine* e) { return llama_model_size(e->model); }
int32_t veyra_model_template(const veyra_engine* e, char* buf, size_t cap) {
    const char* t = llama_model_chat_template(e->model, nullptr); if (!t) { if (cap) buf[0] = 0; return 0; }
    size_t n = std::strlen(t); if (n + 1 > cap) n = cap ? cap - 1 : 0; std::memcpy(buf, t, n); if (cap) buf[n] = 0; return (int32_t)n;
}
int32_t veyra_tokenize(const veyra_engine* e, const char* text, int32_t len, int32_t special, int32_t* out, int32_t cap) {
    int32_t n = llama_tokenize(e->vocab, text, len, out, cap, /*add_special*/ false, /*parse_special*/ special != 0);
    return n == INT32_MIN ? INT32_MIN : n;   // negative = -(needed)
}
int32_t veyra_apply_template(const veyra_engine* e, const char* const* roles, const char* const* contents, size_t n, int32_t add_ass, char* out, int32_t cap) {
    std::vector<llama_chat_message> msgs(n); for (size_t i = 0; i < n; i++) { msgs[i].role = roles[i]; msgs[i].content = contents[i]; }
    const char* t = llama_model_chat_template(e->model, nullptr);
    int32_t r = llama_chat_apply_template(t ? t : "chatml", msgs.data(), n, add_ass != 0, out, cap);
    if (r < 0) return INT32_MIN;              // template not recognised by llama.cpp's built-in formatter
    return r > cap ? -r : r;                  // r > cap: buffer too small, caller retries with -value
}
void veyra_reset(veyra_engine* e) { e->cache.clear(); llama_memory_clear(llama_get_memory(e->ctx), true); }

namespace {
struct Sampler { llama_sampler* s; explicit Sampler(llama_sampler* x) : s(x) {} ~Sampler() { if (s) llama_sampler_free(s); } };
}

int32_t veyra_generate(veyra_engine* e, const char* prompt, int32_t plen, const veyra_gen* g, veyra_piece_cb cb, void* user, const volatile uint8_t* cancel, veyra_stats* st, char* err, size_t errlen) {
    try {
        auto t0 = std::chrono::steady_clock::now(); if (st) std::memset(st, 0, sizeof(*st));
        auto cancelled = [&] { return cancel && *cancel != 0; };
        const int32_t n_ctx = (int32_t)llama_n_ctx(e->ctx);
        std::vector<llama_token> toks((size_t)plen + 16);
        int32_t n = llama_tokenize(e->vocab, prompt, plen, toks.data(), (int32_t)toks.size(), false, true);
        if (n < 0) { toks.resize((size_t)-n); n = llama_tokenize(e->vocab, prompt, plen, toks.data(), (int32_t)toks.size(), false, true); }
        if (n <= 0) { set_err(err, errlen, "the prompt could not be tokenised"); return -1; }
        toks.resize((size_t)n);
        if (n >= n_ctx - 8) { set_err(err, errlen, "the prompt (" + std::to_string(n) + " tokens) does not fit in the context (" + std::to_string(n_ctx) + ")"); return -2; }
        // reuse the KV cache for the common prefix of the previous prompt+answer and this prompt: a follow-up message only pays for its new tokens
        size_t common = 0; while (common < e->cache.size() && common < toks.size() && e->cache[common] == toks[common]) common++;
        if (common >= toks.size()) common = toks.size() - 1;   // always evaluate at least the last prompt token to get fresh logits
        llama_memory_t mem = llama_get_memory(e->ctx);
        if (common < e->cache.size()) { if (!llama_memory_seq_rm(mem, 0, (llama_pos)common, -1)) { llama_memory_clear(mem, true); common = 0; } }
        e->cache.resize(common);
        if (st) { st->prompt_tokens = n; st->prompt_cached = (int32_t)common; }
        auto tp = std::chrono::steady_clock::now();
        for (size_t i = common; i < toks.size();) {
            if (cancelled()) { if (st) st->prefill_ms = ms_since(tp); return 1; }
            // prompt is fed in chunks of at most 128 tokens so that a cancel request is noticed within ~1 s even on a slow CPU (cost: negligible)
            int32_t k = (int32_t)std::min<size_t>((size_t)std::min<int32_t>(e->n_batch, 128), toks.size() - i);
            llama_batch b = llama_batch_get_one(toks.data() + i, k);
            if (llama_decode(e->ctx, b) != 0) { e->cache.clear(); llama_memory_clear(mem, true); set_err(err, errlen, "decoding the prompt failed. " + tail()); return -3; }
            e->cache.insert(e->cache.end(), toks.begin() + i, toks.begin() + i + k); i += (size_t)k;
        }
        if (st) st->prefill_ms = ms_since(tp);
        llama_sampler_chain_params sp = llama_sampler_chain_default_params(); sp.no_perf = true;
        Sampler smp(llama_sampler_chain_init(sp));
        if (g->temperature <= 0.0f) { llama_sampler_chain_add(smp.s, llama_sampler_init_top_k(1)); }
        else {
            if (g->top_k > 0) llama_sampler_chain_add(smp.s, llama_sampler_init_top_k(g->top_k));
            if (g->top_p < 1.0f) llama_sampler_chain_add(smp.s, llama_sampler_init_top_p(g->top_p, 1));
            if (g->min_p > 0.0f) llama_sampler_chain_add(smp.s, llama_sampler_init_min_p(g->min_p, 1));
            llama_sampler_chain_add(smp.s, llama_sampler_init_temp(g->temperature));
        }
        llama_sampler_chain_add(smp.s, llama_sampler_init_dist(g->seed));
        auto td = std::chrono::steady_clock::now(); bool first = true; int32_t gen = 0; int32_t rc = 0;
        const int32_t limit = std::min<int32_t>(g->max_tokens, n_ctx - n - 1);
        while (gen < limit) {
            if (cancelled()) { rc = 1; break; }
            llama_token t = llama_sampler_sample(smp.s, e->ctx, -1);
            if (llama_vocab_is_eog(e->vocab, t)) break;
            char buf[256]; int32_t len = llama_token_to_piece(e->vocab, t, buf, sizeof(buf), 0, false);
            if (len < 0) { std::string big((size_t)-len, '\0'); len = llama_token_to_piece(e->vocab, t, big.data(), (int32_t)big.size(), 0, false); if (len > 0 && cb(big.data(), len, user)) { gen++; break; } }
            else if (len > 0) { if (first && st) { st->first_token_ms = ms_since(t0); first = false; } if (cb(buf, len, user)) { gen++; break; } }
            gen++;
            llama_sampler_accept(smp.s, t);
            llama_batch b = llama_batch_get_one(&t, 1);
            if (llama_decode(e->ctx, b) != 0) { e->cache.clear(); llama_memory_clear(mem, true); set_err(err, errlen, "generation failed. " + tail()); if (st) { st->generated_tokens = gen; st->decode_ms = ms_since(td); } return -4; }
            e->cache.push_back(t);
        }
        if (st) { st->generated_tokens = gen; st->decode_ms = ms_since(td); if (first) st->first_token_ms = ms_since(t0); }
        return rc;
    } catch (const std::exception& ex) { set_err(err, errlen, std::string("native error: ") + ex.what()); return -9; }
    catch (...) { set_err(err, errlen, "unknown native error"); return -9; }
}
