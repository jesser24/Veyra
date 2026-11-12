# Building Veyra

## Local
Prerequisites: Node 22, Rust 1.97.0, [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) (Windows: Visual Studio Build Tools + WebView2; macOS: Xcode command line tools; Linux dev only: webkit2gtk-4.1, libsoup-3.0, GTK 3).
```bash
npm ci
npm run typecheck && npm test      # front-end
cargo test -p veyra-core --locked  # pure logic
npm run tauri dev                  # run the app
npx tauri build --bundles nsis     # Windows installer  -> target/release/bundle/nsis
npx tauri build --bundles app,dmg  # macOS .app + .dmg  -> target/release/bundle/{macos,dmg}
node scripts/verify-artifacts.mjs windows windows-x64   # or: macos macos-arm64
```
Installers can only be built on their own OS. Do not expect a Windows machine to produce macOS artifacts.

## CI (`.github/workflows/ci.yml`)
Runs on every push and pull request: front-end, Rust core, then a `bundle` matrix on `windows-2022`, `macos-15` (Apple Silicon) and `macos-15-intel`. Each job prints tool versions, keeps the full build log as an artifact, runs `verify-artifacts.mjs` (fails if the installer is missing or under 1 MB) and uploads installers with SHA-256 sums. Versions are pinned (Rust 1.97.0, Node 22, lockfiles with `--locked` / `npm ci`).

If a bundle job fails: open the job, then download the `logs-<label>` artifact; the first `error:` line in `tauri-build-<label>.log` is the cause.

## Release
Bump `version` in `src-tauri/tauri.conf.json` (and `package.json`), commit, then `git tag v0.1.0 && git push origin v0.1.0`. `release.yml` rebuilds everything, re-verifies, and creates a **draft** GitHub Release only if all platforms succeeded. You review it and publish.

## Signing and notarisation
- **Windows:** unsigned installers show a SmartScreen warning. A code-signing certificate (or Azure Trusted Signing) is needed; not configured yet.
- **macOS:** unsigned apps are blocked by Gatekeeper. You need an Apple Developer account ($99/year). Add repository secrets `APPLE_CERTIFICATE` (base64 .p12), `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD` (app-specific password), `APPLE_TEAM_ID`; Tauri then signs and notarises during `tauri build`. Without them the release workflow still builds, **unsigned**. Never commit these values.

## Not yet verified
No GitHub Actions run of these workflows has happened at the time of writing, and neither the Windows nor the macOS build has been executed on a real OS.
