# Veyra — Roadmap and verification status

Legend: **Done+verified** = implemented and checked as described. **Compiles** = builds in CI-equivalent conditions but not run on the target OS. **Planned** = not started. Nothing marked Planned is claimed to work.

## Phase 1 — Foundations
| Item | Status | Evidence / limit |
|---|---|---|
| Architecture decision | Done | ARCHITECTURE.md |
| Workspace, CI, release workflow | Compiles | Workflows written; first GitHub run pending |
| Toggle geometry (edges, corners, margins, multi-monitor, DPI) | Done+verified (logic) | 8 unit tests incl. mixed DPI, unplugged monitor |
| Settings persistence (atomic, corrupt-safe, clamped) | Done+verified | 4 unit tests |
| Toggle window, drag, snap, click, panel placement | Compiles | `cargo check` on Linux; **not run on Windows or macOS** |
| Toggle appearance settings (size, shape, colour, opacity, shadow, reflection, animations) | Compiles | UI built; visual check on real OS pending |
| Native blur / Liquid-Glass backdrop | Planned | Needs Mica/Acrylic/NSVisualEffectView |
| macOS floating NSPanel behaviour | Planned | |
| Panel: compact / expanded, history drawer | Planned | Phase 2 with chat |
| Encrypted storage (SQLCipher, Argon2id), master password, backups | Planned | |
| Accessibility basics (keyboard focus on toggle, Esc closes, reduced motion) | Partial | Screen-reader test pending; custom shortcuts planned |

## Phase 2 — Local AI
llama.cpp integration, model manager (hardware detection, hash-verified resumable download, test generation, measured performance), streaming chat, cancel, regenerate, conversations, titles, search, folders, favourites, archive, trash, export, context summarisation. **Planned.**

## Phase 3 — Advanced features
Attachments and local document extraction (PDF, DOCX, XLSX, CSV, PPTX, archives without execution), document generation, image generation/editing (pluggable engines, local first), sandboxed code execution, manual web search with sources, advanced history. **Planned.**

## Phase 4 — Privacy
Network ledger and dashboard, measurement protocol and report, privacy centre, exportable report, security review. **Planned.**

## Phase 5 — Distribution
Windows and macOS tests on real systems, installers, signed/notarised macOS build, optional update check with user approval and rollback, full documentation set. **Planned** (installer generation is already wired in CI).

## Out of scope by design
Voice (microphone, dictation, speech), telemetry, ads, cloud sync, any Veyra server.
