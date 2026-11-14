# Veyra — Roadmap and verification status (2026-11-14)

Every item carries the **highest level actually reached**:
**L1** code written · **L2** unit tests · **L3** tests/build on the 3 CI platforms (GitHub runners) · **L4** real integration on CI (real model, real window) · **L5** checked by a person on a real machine.
**No item is L5 yet.** Nothing has been run by a person on a Dell Latitude 7490, a Windows desktop session or a Mac; CI runners are virtual machines (e.g. the "Apple M1 (Virtual)", 3 cores) and their speeds say nothing about your computer.

## Local AI engine
| Item | Level | Evidence / limit |
|---|---|---|
| llama.cpp b12336 built from source, **linked in-process** (no llama-server, no HTTP, no child process) | **L4** | `veyra-bench --check` on windows-2022, macos-15 (M1 VM), macos-15-intel: "child processes: []", all checks passed (run 43602562921) |
| Real generation, streaming, cancel, unload/reload (Qwen3-0.6B Q8_0, downloaded through Veyra's gateway, SHA-256 checked) | **L4** | same run. Windows 2-core: 42 tok/s decode; M1 VM: 56–67 tok/s; Intel i7-8700B VM: 32 tok/s |
| KV-cache reuse between turns | **L4** | 372 of 419 prompt tokens reused; follow-up prefill 456 ms vs 2829 ms (Windows). On macOS-Intel the follow-up was not faster (2853 ms): reuse works, timing there is noisy |
| Memory released on unload | **L4** | Windows 1152 → 22 MB, macOS Intel 1153 → 52 MB, M1 VM 1773 → 109 MB |
| Cancel during a long prefill | **L4**, being improved | took 1.5–2.7 s after the stop request; prefill chunks reduced from 512 to 128 tokens (not yet re-measured) |
| Auto-load of the selected verified model at start, no reload when the chat closes/reopens, optional idle unload | **L1–L2** | Logic in `src-tauri/src/ai.rs`; no automated end-to-end test of the app's start-up path; not seen on a real machine |
| Load-error messages with a corrective action | L1 | |
| Thinking: off by default, optional "Reasoning" mode, reasoning text never shown | L2 | `ThinkSplitter` unit tests; quality impact not measured |
| Speed study (threads, batch, ctx, quantisation, flash attention, KV q8) | **L1** | Tooling exists (`veyra-bench --tune`, manual `benchmark.yml`); **no comparison of Qwen3-0.6B Q8_0 / 4B Q4_K_M / 4B Q5_K_M has been run yet**, so no recommendation beyond the defaults (ctx 4096 on CPU, thinking off, physical-core threads) is claimed. Dell performance: **not measured** |
| Static CRT on Windows, Metal/Accelerate on macOS | **L3/L4** | CI links and runs on all three |
| macOS Intel / Apple Silicon installers contain the engine | **L3** | Bundle jobs green; self-test checks the library is linked and no child exists |

## Interface
| Item | Level | Evidence / limit |
|---|---|---|
| Chat window: header only (logo, history, settings), conversation, fixed input, no tabs | **L2** (browser) | Exercised in Chromium with a mocked backend at 340×420 … 900×700, light & dark: send, stream, stop, history, attachments, setup (scripted, 31 checks). **Not run inside the real Tauri window** and not seen by a person |
| Markdown: code, tables (alignment, escaped pipes), lists; no token counters/tech info under replies | L2 | vitest (14 tests) + screenshots |
| Sliding history (search, rename, archive, trash, restore, delete forever), overlays without moving the input | L2 | same browser run |
| Resizable chat with minimum size (340×420) and remembered size | L1 | Native min-size/resize persistence written, not run on a real window |
| Application window (10 sections) sharing backend state/events with the chat | L1–L2 | Sections render in Chromium; backend commands unit-tested where logic exists; the two real windows have not been opened together |
| Settings button opens the application; "open in chat" from Conversations | L1 | `open_app` / `open_chat` commands; real-window self-test of these not written |
| Light/dark/system, accent, text size, density, opacity (≥ 88 %), animations, Enter behaviour | L2 | Applied as CSS variables; screenshots |
| Toggle settings (shape, size, colour, opacity, margin, snap, second click, reset position) | L1–L3 | Existing from earlier steps |
| Native Mica / Acrylic / vibrancy | **Not implemented** | Windows are deliberately near-opaque; the Appearance page says so |

## Attachments
| Item | Level | Evidence |
|---|---|---|
| Text/code/JSON/XML/YAML, CSV (column statistics), PDF, DOCX, PPTX, XLSX extraction | **L2** | 11 unit tests in `veyra-docs`, plus a probe on real PDF/DOCX/XLSX files created by other libraries |
| ZIP and 7z: listing, bounded in-memory reading, path-traversal entries ignored, bomb limits | L2 (ZIP), **L1 (7z: no real 7z fixture tested)** | |
| RAR | **Not supported** (stated in the UI) | |
| Images: preview only. **The local Qwen3 models cannot see images and there is no OCR**; the prompt tells the model so | L2 | |
| Attachments encrypted with the conversation, linked to messages, removed with them | L2 | store test |
| Picker, drag-and-drop, paste, multi-select, remove before send, progress, limits | L2 (browser) | scripted checks; real OS drag-and-drop from Explorer/Finder not tested |

## Security and storage
| Item | Level | Evidence |
|---|---|---|
| Master password removed from the app (no login/creation screen, no lock) | L2 | Old vault commands deleted; legacy vault files from development builds are moved aside |
| Random key in the OS credential store (Windows Credential Manager / macOS Keychain), 0600 file fallback | L2 (file store, logic), **L1 (OS stores)** | The Keychain/Credential Manager paths have not been exercised on a real session; CI uses the fallback if unavailable. The Security page reports which is active |
| Backup / verify / restore with the system key; key export/import for another computer | **L2** | Test: backup unreadable with another key, restorable after import, wrong passphrase refused. No UI-level test |
| Installation key `Veyra@2024` (first-run step; hash compare; **not** cryptographic, **not** a licence) | L2 | Limits documented in docs/SECURITY.md. No installer-time prompt (cannot be verified without a Windows run) |

## Earlier phases (unchanged)
Toggle geometry (L2), settings persistence (L2), network gateway + ledger + dashboard (L2/L3), model catalogue with verified SHA-256 and resumable download (L2/L4), context summarisation (L2), CI for 3 installers (L3). Phase 3+ items not yet started: code execution sandbox, web search, image generation, document generation, installers' signing/notarisation, accessibility audit with a screen reader.

## Out of scope by design
Voice, telemetry, ads, cloud sync, any Veyra server.
