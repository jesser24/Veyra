<p align="center"><img src="assets/brand/veyra-icon-1024.png" width="120" alt="Veyra"></p>

# Veyra

*Your intelligence, always within reach.*

Veyra is an independent desktop AI assistant for Windows and macOS. It lives behind a small floating toggle that you place anywhere on your screen, and it is designed to run **locally** (llama.cpp + a single general-purpose model), with no Veyra server, no telemetry and no account.

> **Status: early development (v0.1.0 – foundations).** Read [docs/ROADMAP.md](docs/ROADMAP.md) for exactly what works today and what does not. Nothing is claimed as working unless it is listed there as *implemented* and *verified*.

## What exists today
- **Veyra Chat** (opened by the toggle): header, conversation, fixed input. Markdown with code blocks and tables, streaming, attachments (picker, drag-and-drop, paste), sliding history (search, rename, archive, delete), freely resizable with real responsive layouts.
- **Veyra Application** (opened from the chat's settings button): AI, Models, Conversations, Appearance, Toggle, Documents, Network & Privacy, Security & storage, Backups, Advanced — same data and state as the chat.
- **Local AI inside the Veyra process**: llama.cpp is linked in (no `llama-server`, no local server, no child process); Qwen3 models downloaded on request, SHA-256-verified, loaded automatically at start.
- **No master password**: the data key is random and kept by the OS credential store (Windows Credential Manager / macOS Keychain); encrypted conversations and attachments; backups; key export for moving computers. See [docs/SECURITY.md](docs/SECURITY.md).
- Floating edge-tab toggle with full position/appearance settings; zero telemetry; every Internet connection goes through one recorded gateway.
- CI that builds Windows, macOS Apple Silicon and macOS Intel installers, links the engine, and runs real inference on the runners.

## Documentation
| | |
|---|---|
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | Technology comparison, decisions, security and privacy model |
| [docs/ROADMAP.md](docs/ROADMAP.md) | Every requirement, its phase and its verification status |
| [docs/SECURITY.md](docs/SECURITY.md) | What is protected, what is not, backups, the installation key |
| [docs/BENCHMARK.md](docs/BENCHMARK.md) | Reproducible performance measurement |
| [docs/BUILD.md](docs/BUILD.md) | Local build, CI, releases, signing and notarisation |

## Quick start (development)
```bash
npm ci
npm run tauri dev
```
Requirements: Node 22, Rust 1.97, CMake + a C++ toolchain (to build the inference library, see BUILD.md) and the [Tauri system prerequisites](https://v2.tauri.app/start/prerequisites/). Details in [docs/BUILD.md](docs/BUILD.md).

## Name
"Veyra" is a working name. Pending third-party trademark filings for VEYRA / VEYRA AI exist in other classes (see ARCHITECTURE.md §9); this is not a legal clearance.
