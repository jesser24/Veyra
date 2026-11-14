// C API of the Veyra shim over libllama (pinned llama.cpp). Everything is synchronous; the Rust side calls it from a worker thread.
#pragma once
#include <stddef.h>
#include <stdint.h>
#ifdef __cplusplus
extern "C" {
#endif

typedef struct veyra_engine veyra_engine;
typedef struct {
    const char* model_path;
    int32_t n_ctx, n_batch, n_ubatch, n_threads, n_threads_batch;
    int32_t flash_attn;      // -1 auto, 0 off, 1 on
    int32_t kv_q8;           // 1: quantise the KV cache to Q8_0 (half the memory), 0: F16
    int32_t n_gpu_layers;    // 0 = CPU only; >0/-1 offload when a GPU backend (Metal) was compiled in
    int32_t use_mmap;
} veyra_config;
typedef struct { int32_t max_tokens; float temperature, top_p, min_p; int32_t top_k; uint32_t seed; } veyra_gen;
typedef struct {
    int32_t prompt_tokens;      // tokens in the prompt
    int32_t prompt_cached;      // of which reused from the KV cache (not recomputed)
    int32_t generated_tokens;
    double prefill_ms;          // time spent processing the new prompt tokens
    double decode_ms;           // time spent generating
    double first_token_ms;      // start of the call -> first visible piece
} veyra_stats;
// return non-zero from the callback to stop generating
typedef int (*veyra_piece_cb)(const char* piece, int32_t len, void* user);

const char* veyra_llama_version(void);
veyra_engine* veyra_engine_new(const veyra_config* cfg, char* err, size_t errlen);
void veyra_engine_free(veyra_engine*);
int32_t veyra_n_ctx(const veyra_engine*);
int32_t veyra_n_ctx_train(const veyra_engine*);
uint64_t veyra_model_bytes(const veyra_engine*);
// writes the model's embedded chat template (or "") into buf
int32_t veyra_model_template(const veyra_engine*, char* buf, size_t cap);
// returns token count, or -(needed) if cap is too small, or INT32_MIN on error
int32_t veyra_tokenize(const veyra_engine*, const char* text, int32_t text_len, int32_t special, int32_t* out, int32_t cap);
// applies the model's own chat template; returns bytes written, -(needed) if too small, INT32_MIN if the template is unsupported
int32_t veyra_apply_template(const veyra_engine*, const char* const* roles, const char* const* contents, size_t n, int32_t add_assistant, char* out, int32_t cap);
// 0 = finished (EOG/limit/stopped by callback), 1 = cancelled, <0 = error (message in err)
int32_t veyra_generate(veyra_engine*, const char* prompt, int32_t prompt_len, const veyra_gen*, veyra_piece_cb, void* user, const volatile uint8_t* cancel, veyra_stats*, char* err, size_t errlen);
// forget the KV cache (e.g. after an error)
void veyra_reset(veyra_engine*);

#ifdef __cplusplus
}
#endif
