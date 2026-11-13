# Measuring Veyra on your own computer (Dell Latitude 7490 or any other)

CI numbers come from GitHub runners and say nothing about your machine. This benchmark runs the **real** llama-server with a **real** Qwen3 model and prints only what it measured.

## Quick way (installed Veyra)
Use the **Artificial intelligence** tab: Download → Load → **Test generation**. It shows tokens/s, first-token delay, load time and RAM measured on your computer.

## Full benchmark (download, load, first token, tokens/s, RAM, CPU, cancel, long prompt, unload/reload)
1. Install Rust (https://rustup.rs) and clone the repository.
2. Build the engine once (needs CMake and a C++ toolchain: Visual Studio Build Tools on Windows, Xcode tools on macOS):
   `bash scripts/build-llama-server.sh work llama-out`  (Windows: run it from Git Bash)
3. Run (the model is downloaded from Hugging Face and checked against its published SHA-256; ≈ 2.9 GB for the recommended one):
   ```
   cargo run --release -p veyra-ai --bin veyra-bench -- --server llama-out/llama-server.exe --dir models --model qwen3-4b-q5_k_m --ctx 8192 --runs 3 --long --out report.json
   ```
   Other ids: `qwen3-4b-q4_k_m`, `qwen3-8b-q4_k_m`, `qwen3-0.6b-q8_0` (see `crates/veyra-ai/src/catalog.rs`). Add `--threads N` to compare thread counts.
4. Close heavy applications first and plug the laptop in (Windows: *Best performance* power mode); repeat the run 2–3 times and keep `report.json`.

## What the figures mean
- *download+SHA-256 verification*: network time plus hashing time (hashing alone if the file was already there).
- *load*: from process start until the engine answers with Veyra's key; the first load after a reboot is slower (disk cache cold).
- *first token*: prompt processing + first decode step. *tok/s*: the engine's own `predicted_per_second`.
- *CPU %* is percent of ONE core (400 % = four cores busy). *RAM* is the engine process's resident memory.
- *cancel*: time between the stop request and the end of the stream (must stay under 3 s).
- *long prompt*: one prompt filling ≈ 70 % of the context; shows how prompt processing time grows.
- `--check` makes the run fail on empty/non-streamed answers, a wrong `17 + 25`, slow cancellation, or a failed generation after reload.

Send `report.json` back to compare against the figures the project will publish; the project never states a speed for your machine that was not measured on it.
