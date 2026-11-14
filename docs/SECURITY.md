# Veyra — security model and its limits

## What protects your data
- **Conversations**: SQLCipher (AES-256, page-level HMAC). **Attached files**: XChaCha20-Poly1305, one file per attachment, authenticated (tampering is detected).
- **The key** is 256 random bits created on first start. It is stored by the operating system, not derived from anything you type:
  - Windows: Credential Manager (encrypted with DPAPI, bound to your Windows account).
  - macOS: Keychain (bound to your user account).
  - If no credential store is available (for example a headless session): a file readable only by your account (`0600` on Unix). The Security page says which one is in use. This fallback is **weaker** and is reported as such.
- **No master password.** Veyra opens directly. There is no login screen and no lock.

## What this does and does not protect against
| Threat | Protected? |
|---|---|
| Someone copies your data folder / disk / cloud-synced folder / backup | Yes: unreadable without the key |
| Someone signs in to your OS account, or malware runs as you | **No**: it can ask the credential store for the key exactly as Veyra does |
| Someone has your unlocked computer | **No** |
| Disk theft with full-disk encryption off | Partly (see first row) — enable BitLocker / FileVault anyway |
Removing the master password trades "something you know" for convenience; the protection is now the strength of your OS account and disk encryption. This is a deliberate product decision, not an improvement in cryptographic strength.

## Backups and moving to another computer
- Backups (folders under the data directory, with a SHA-256 manifest) contain the encrypted database and attachments. **They do not contain the key.**
- On the **same computer and account**, a backup restores with no extra step (`Backups → Restore`; your current data is moved aside, never deleted).
- On **another computer** the key is different, so the backup is unreadable there. Veyra does **not** claim otherwise. To move: on the old computer, *Security → Export key* (passphrase of 10+ characters; Argon2id + authenticated encryption; saved as a JSON file). On the new computer: *Import key*, then restore the backup. Import is refused if the new Veyra already holds conversations (it would make them unreadable). Whoever has both the export file and its passphrase can read your backups: store them separately.
- Tested in CI: backup unreadable with a different key; restorable after import; wrong passphrase refused (`crates/veyra-store/tests/store.rs`).

## The installation key (`Veyra@2024`)
Asked once, at first configuration. It is compared by SHA-256 so that it is not stored in clear, but **any universal key shipped inside an installer or executable can be extracted** (strings, decompilation, or simply reading the code). Therefore:
- it is **not** an encryption key and is never used to derive or wrap any data key;
- it is **not** a licence system and does not prevent copying or use by someone who has the installer;
- it plays no role in protecting your data, which relies only on the system-protected key above;
- nothing asks for it after the first configuration.
It was implemented as an in-app first-run step; a custom page inside the Windows installer was not attempted (it cannot be verified without running the installer on Windows).

## Network
Veyra makes no connection except model downloads you start (Hugging Face over HTTPS), recorded in a local ledger (host, time, size, result — never paths or content). No telemetry. The inference engine is in-process: there is no local server to attack. Connections made by other programs or the web-view engine are not visible to Veyra.

## Attachments (untrusted input)
Parsed in memory with bounded sizes; ZIP/7z are never extracted to disk (no path traversal possible), unsafe entry names are ignored, total/entry size, entry count and compression ratio are capped (decompression bombs), nothing is executed; SVG is shown as a picture (scripts do not run) and read as text; RAR is refused; parser panics are caught and reported as "malformed file". The web view's CSP forbids remote scripts and `connect-src` beyond Tauri IPC.

## Not done
Sandboxed code execution, signed installers/notarisation, an external security review.
