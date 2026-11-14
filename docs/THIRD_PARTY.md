# Third-party components embedded in Veyra

| Component | Version | License | How it is obtained |
|---|---|---|---|
| llama.cpp (libllama + ggml, static libraries linked into Veyra) | tag b12336, commit `cfb9d4b754616ca47d165fa1ff8a7a80c30847e3` | MIT (+ bundled third-party licenses listed in its `licenses/`) | Built from source in CI at that exact commit (the commit hash is checked after checkout). **No prebuilt binary is downloaded**, no server and no web UI are built, no OpenSSL/curl: the engine never opens a connection; Veyra downloads models itself through its network gateway. Licenses are collected by `scripts/build-libllama.sh`. |
| Qwen3 models (not bundled) | see `crates/veyra-ai/src/catalog.rs` | Apache-2.0 | Downloaded on demand by the user from the official Hugging Face repositories; SHA-256 checked. |


Rust dependencies: see `Cargo.lock`; SQLCipher and OpenSSL (vendored) are BSD-style / Apache-2.0 licensed.

Document parsing (`veyra-docs`): `zip`, `quick-xml`, `calamine`, `pdf-extract`, `sevenz-rust2`, `imagesize` (MIT/Apache-2.0 family; see Cargo.lock). Key storage: `keyring`.
