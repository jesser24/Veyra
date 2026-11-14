# Veyra — Architecture

Status: accepted for Phase 1. Comparisons are qualitative engineering judgement from documented capabilities, **not benchmarks**; no memory/CPU figure is claimed until measured.

## 1. Technology comparison

| Criterion | **Tauri 2 (Rust + web UI)** | Electron | Native (WinUI/WPF + SwiftUI/AppKit) |
|---|---|---|---|
| Idle memory / install size | Low: system WebView (WebView2 / WKWebView), small installer | Highest: bundled Chromium | Lowest, but two apps |
| Transparent floating windows | Supported on both (macOS needs `macOSPrivateApi`) | Supported | Best control |
| Native blur / vibrancy | Via `window-vibrancy` (Mica/Acrylic, NSVisualEffectView) | Built-in on both | Native |
| Shared code Windows/macOS | One Rust core + one UI | One JS code base | Two code bases (double the work and bugs) |
| Local AI (llama.cpp) | Rust spawns/links native code; process isolation easy | Possible, heavier | Possible |
| Process isolation / security | Rust backend; UI cannot touch the OS except through audited commands; capability system | Needs strict contextIsolation discipline | Native |
| Build/CI reliability | Proven here: MyPinboard shipped through the same stack on GitHub Actions | Mature | Needs Windows and macOS toolchains per app |
| Maintenance | Two mainstream stacks | One stack | Two stacks |

**Decision: Tauri 2 + Rust core + React/TypeScript.** Electron is rejected because its resident Chromium costs RAM that the local model needs (the reference machine has an i5-8350U with 24 GB RAM, where the CPU is the limit). Fully native is rejected because it doubles the code for a two-platform product built by a small team. Windows 10/11 and macOS (Apple Silicon first, Intel via CI) are targets; **no compatibility is claimed until tested on real systems.**

## 2. Layout
```
crates/veyra-core/   pure Rust: toggle geometry, settings (tested on Linux, no OS dependency)
src-tauri/           Tauri shell: windows, commands, persistence wiring
src/                 React UI: Toggle window, Panel window
assets/brand/        logo, mark, app icon source
scripts/             artifact verification
.github/workflows/   ci.yml (every push), release.yml (tags)
docs/
```
Planned crates (Phase 2+): `veyra-ai` (llama.cpp supervisor, model manager), `veyra-store` (encrypted SQLite, attachments, backups), `veyra-net` (the only code allowed to open a socket, with a request ledger), `veyra-sandbox` (code execution), `veyra-docs` (local extraction/generation).

## 3. Windows
- `toggle`: frameless, transparent, always-on-top (configurable), no taskbar entry. Window = visible toggle + 12 dip transparent halo for the shadow. A press without movement is a click; movement hands the drag to the OS (`startDragging`), which is correct across monitors and DPI. When no move event arrives for 220 ms the drag is over: the rectangle is snapped (edge/corner within a configurable distance), saved and re-applied.
- `panel`: frameless, resizable, created on first open, placed beside the toggle towards the screen centre and clamped to the work area.
- macOS: the app runs as an accessory (no Dock icon). **Limitation:** a plain window cannot float above other apps' full-screen Spaces or avoid activating on click; that needs an NSPanel and is a Phase 1 task still to do.
- Display changes: the placement is stored as fractions of the free travel range of a monitor, so it survives resolution/DPI changes; a 3-second local comparison of the monitor layout re-homes the toggle after plug/unplug/wake. (This is a local check, no network.) Native display-change hooks may replace it.
- **Not real yet:** blur of the desktop behind the toggle. A web `backdrop-filter` cannot see the desktop through a transparent window; true blur needs native Mica/Acrylic/vibrancy. No "blur" control is exposed until it works.

## 4. Local AI (decisions, 2026-11-13)

**Engine (revised 2026-11-14): in-process.** llama.cpp (tag b12336, commit `cfb9d4b7…`) is compiled **from source in CI** at the pinned commit as *static libraries* and linked into the Veyra executable through a small C++ shim (`crates/veyra-llama`, `csrc/veyra_llama.cpp`) and a safe Rust wrapper. There is **no `llama-server`, no HTTP server, no loopback port, no helper executable and no child process**: Tauri → Rust (`veyra-ai::engine`) → shim → libllama → GGUF, all in one process. This replaces the earlier supervised-child-process design (kept in git history), because the requirement is that Veyra shows up as a single process and does not expose any local server. Consequences, stated plainly: (1) a crash inside native inference code now takes the whole app down (mitigated by input validation, a pinned and CI-tested llama.cpp version, and `panic=unwind` + `catch_unwind` at the FFI edge for Rust-side failures only; a native abort cannot be caught); (2) memory is returned by dropping the model and context (measured on CI: 1151 MB → 21 MB after unload); (3) cancellation is an atomic flag checked between prompt batches and between decode steps (a cancel during a long prefill can take as long as one batch, measured ≈ 2.6 s on a 2-core CI runner); (4) llama.cpp's worker threads exist inside the Veyra process, they are threads, not processes.

**Process model.** Single process. `veyra-bench --check` and the app self-test assert that no child process exists and that the library is linked (`IS_STUB == false`). On Windows/macOS the same check is run by CI on real runners; *looking at Task Manager / Activity Monitor on a real desktop session has not been done* (see ROADMAP).

**Loading.** The selected model is loaded once per app session at start-up when it is downloaded **and** SHA-256-verified (`ai.auto_load`, default on); closing the chat window never unloads it; reopening reuses the same instance. Optional idle unload (`ai.idle_unload_minutes`, off by default). Load errors are translated into an action (smaller context, smaller model, re-download).

**Speed.** Context defaults to 4096 on CPU-only machines (prompt prefill is the dominant cost: ≈ 89 tok/s on a 2-core CI runner, so 8k tokens would be ≈ 90 s) and 8192 on Apple Silicon. The KV cache is reused between turns when the new prompt extends the previous one (measured: 372 of 419 prompt tokens reused, follow-up prefill 556 ms vs 2633 ms). Thinking is **off by default** (Qwen3 chat template with an empty think block); a Reasoning mode exists in settings and the reasoning text is never shown. Threads default to physical cores. Batch, flash attention, 8-bit KV cache are settings. Measured numbers on CI are *not* the Dell's numbers (docs/BENCHMARK.md).

**Model choice (verified 2026-11-12 against the official model cards).** Candidates: Qwen3-4B (Apache-2.0, official GGUF, 32K context, thinking/non-thinking, strong multilingual incl. French), Qwen3-8B (better reasoning/code, ~2× slower on CPU), Qwen3-0.6B (test only). **Default: Qwen3-4B Q5_K_M** (2.89 GB, SHA-256 in the catalog): on a 4-core/AVX2 laptop it should be usable without being the slowest option, and 24 GB RAM leaves ample headroom; Q4_K_M is the low-RAM fallback; 8B is offered, never default, on CPU. **Qwen3.5-4B** (Feb 2026; hybrid architecture, 262K context, vision) is listed as a *candidate only*: no official GGUF, community conversions unverified; it can replace the default only after (a) llama.cpp support is confirmed at the pinned version, (b) a GGUF hash from a trusted source is recorded, (c) `veyra-bench` shows it is not slower on the reference machine. Speeds are never promised: they are measured (docs/BENCHMARK.md).

**Model states.** available → partial → downloaded (a file on disk, **not usable**) → verified (full SHA-256 matched for exactly this file; invalidated if size/mtime change) → loaded → engine ready (key accepted). Loading is refused unless verified.

**Context.** The database keeps every message. The model receives `[system] + [summary of older turns] + [latest turns that fit ctx − 1024]`; token counts come from the loaded model's own `/tokenize`. The summary is generated by the same model, stores how many messages it covers, and is extended (not silently stale) when the window moves. The model still has no memory outside this window.

**Chat persistence.** Conversations live in the SQLCipher database. The key is a random 256-bit data key kept by the operating system's credential store (no master password; see SECURITY.md); HKDF derives separate sub-keys for the database and for attachment files. Partial answers are saved every 2 s; a reply interrupted by a crash is marked as an error at next start.

**Two windows, one state.** The chat window (`w=panel`) and the application window (`w=app`) are two webviews of the same process talking to the same Rust state. The backend owns everything (settings, engine state, conversations, downloads) and broadcasts events (`settings-changed`, `ai-state`, `ai-download`, `conversations-changed`, `chat-token`…) to every window, so changing the theme, loading a model or renaming a conversation in one is immediately visible in the other. There is no second store.

**Attachments.** Files arrive in the backend as a raw binary request body, are analysed in memory by `veyra-docs` (bounded; archives never touch the disk), kept in memory until the message is sent, then stored encrypted with the conversation; the extracted text is put in front of the user's message in the prompt, trimmed to the context budget. Images are previewed only: the local Qwen3 models are text-only and there is no OCR yet.

## 5. Security model
- UI → Rust only through named commands; capability file grants the UI nothing else (no fs, no shell, no http).
- CSP: no remote scripts, styles, fonts or images; `connect-src` limited to Tauri IPC.
- Storage: SQLCipher database, XChaCha20-Poly1305 for attachments, system-protected random key (Windows Credential Manager/DPAPI, macOS Keychain, 0600 file fallback), passphrase-wrapped key export for moving to another computer. No master password. Details and limits: [SECURITY.md](SECURITY.md).
- Code execution: never in the main process. Windows: Job Objects (CPU/memory/time) + restricted token/AppContainer. macOS: Seatbelt profile (`sandbox-exec` is deprecated but functional) + resource limits. A child process alone is not a sandbox; limits of each mechanism will be documented in `docs/SECURITY.md`.

## 6. Privacy and network
Phase 1 makes **no** network connection. Rule for later phases: one module (`veyra-net`) is the only code that opens sockets; every request is recorded in a local ledger (domain, purpose, time, bytes, latency, error class) that feeds the Network & Privacy panel and the exportable report. TLS validation is never disabled; system proxy settings are respected; no traffic camouflage, no proxy/filter bypass. Anything not observable by the application (what the OS or network sees) is not claimed; measurement uses external tools (Wireshark, OS tools) in a test environment you are authorised to use.

## 7. Distribution
GitHub Actions on real runners (`windows-2022`, `macos-15` for Apple Silicon, `macos-15-intel` for Intel). No cross-compilation. A job fails unless the installer exists and is over 1 MB. Details: [BUILD.md](BUILD.md).

## 8. What was reused from MyPinboard
Concepts only, re-implemented: placement stored independently of resolution, monitor fallback to primary without overwriting the saved choice, lazy creation of the second window, single-instance, CI structure (web / core / native jobs), artifact checks. Not reused: Windows-only Win32 code and cursor-polling drag (replaced by the OS drag for cross-platform behaviour). MyPinboard was only read, never modified.

## 9. Name check (not legal advice)
A web search found a pending US application for VEYRA (Class 9, smart-home/security hardware, filed April 2025) and for VEYRA AI (Class 42, advertising-technology SaaS, filed March 2026, owner Magnite). Different fields, but both are close enough that a proper clearance search (USPTO/EUIPO/INPI, domains) is advisable before public distribution.
