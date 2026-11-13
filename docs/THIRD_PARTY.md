# Third-party components embedded in Veyra

| Component | Version | License | How it is obtained |
|---|---|---|---|
| llama.cpp (`llama-server`) | tag b12336, commit `cfb9d4b754616ca47d165fa1ff8a7a80c30847e3` | MIT (+ bundled third-party licenses listed in its `licenses/`) | Built from source in CI at that exact commit (the commit hash is checked after checkout). **No prebuilt executable is downloaded**, and the optional prebuilt web UI is disabled. Built without OpenSSL and without curl: the engine never opens an outgoing connection; Veyra downloads models itself through its network gateway. |
| Qwen3 models (not bundled) | see `crates/veyra-ai/src/catalog.rs` | Apache-2.0 | Downloaded on demand by the user from the official Hugging Face repositories; SHA-256 checked. |

The license texts shipped next to the binary are in `resources/llama/` (`LLAMA_CPP_LICENSE`, `LICENSE-*`, `LLAMA_CPP_VERSION`).
Rust dependencies: see `Cargo.lock`; SQLCipher and OpenSSL (vendored) are BSD-style / Apache-2.0 licensed.
