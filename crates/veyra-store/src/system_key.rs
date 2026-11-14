//! Password-less protection of the data key.
//!
//! The data-encryption key (DEK) is random and lives in the operating system's credential store, bound to the user's account:
//!   * Windows: Credential Manager (DPAPI), * macOS: Keychain, * other systems / store unavailable: a file with owner-only permissions (reported as weaker).
//! Consequences (documented in docs/SECURITY.md): the data can be read by anything running as the same OS user, and a copy of the encrypted
//! database alone is useless on another computer. Moving to another computer therefore needs an explicit, passphrase-protected key export
//! (`export_key` / `import_key`), which is NOT a startup password: it is only used when exporting or importing.
use crate::crypto::{self, KdfParams, Key};
use crate::vault::Unlocked;
use crate::{Error, Result};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub trait KeyStore: Send + Sync {
    fn get(&self) -> Result<Option<[u8; 32]>>;
    fn set(&self, key: &[u8; 32]) -> Result<()>;
    /// "os" (credential store) or "file" (owner-only file): shown to the user, never hidden.
    fn kind(&self) -> &'static str;
}

/// Owner-only file. Used for tests, Linux development and as an explicit fallback; protection = file permissions only.
pub struct FileKeyStore { pub path: PathBuf }
impl KeyStore for FileKeyStore {
    fn get(&self) -> Result<Option<[u8; 32]>> {
        match std::fs::read_to_string(&self.path) { Ok(t) => { let b = crypto::unhex(t.trim()).ok_or_else(|| Error::Format("key file is corrupt".into()))?; Ok(Some(b.try_into().map_err(|_| Error::Format("key file has the wrong length".into()))?)) }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None), Err(e) => Err(e.into()) }
    }
    fn set(&self, key: &[u8; 32]) -> Result<()> {
        if let Some(d) = self.path.parent() { std::fs::create_dir_all(d)?; }
        let tmp = self.path.with_extension("tmp"); write_private(&tmp, &crypto::hex(key))?; std::fs::rename(tmp, &self.path)?; Ok(())
    }
    fn kind(&self) -> &'static str { "file" }
}
fn write_private(p: &Path, content: &str) -> std::io::Result<()> {
    #[cfg(unix)] { use std::io::Write; use std::os::unix::fs::OpenOptionsExt; let mut f = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(p)?; f.write_all(content.as_bytes()) }
    #[cfg(not(unix))] { std::fs::write(p, content) }
}

/// Windows Credential Manager / macOS Keychain through the `keyring` crate.
#[cfg(any(windows, target_os = "macos"))]
pub struct OsKeyStore { service: String, user: String }
#[cfg(any(windows, target_os = "macos"))]
impl OsKeyStore { pub fn new(service: &str, user: &str) -> Self { OsKeyStore { service: service.into(), user: user.into() } } fn entry(&self) -> Result<keyring::Entry> { keyring::Entry::new(&self.service, &self.user).map_err(|e| Error::Format(format!("credential store: {e}"))) } }
#[cfg(any(windows, target_os = "macos"))]
impl KeyStore for OsKeyStore {
    fn get(&self) -> Result<Option<[u8; 32]>> {
        match self.entry()?.get_password() { Ok(h) => { let b = crypto::unhex(h.trim()).ok_or_else(|| Error::Format("stored key is corrupt".into()))?; Ok(Some(b.try_into().map_err(|_| Error::Format("stored key has the wrong length".into()))?)) }
            Err(keyring::Error::NoEntry) => Ok(None), Err(e) => Err(Error::Format(format!("credential store: {e}"))) }
    }
    fn set(&self, key: &[u8; 32]) -> Result<()> { self.entry()?.set_password(&crypto::hex(key)).map_err(|e| Error::Format(format!("credential store: {e}"))) }
    fn kind(&self) -> &'static str { "os" }
}

/// The best store available on this system. `fallback` is used (and reported) only if the OS store cannot be used at all, and never silently:
/// the caller shows `kind()` to the user.
pub fn default_store(service: &str, fallback_file: &Path) -> Box<dyn KeyStore> {
    #[cfg(any(windows, target_os = "macos"))]
    { let s = OsKeyStore::new(service, "data-key"); if s.get().is_ok() { return Box::new(s); } }
    let _ = service; Box::new(FileKeyStore { path: fallback_file.to_path_buf() })
}

/// Opens the data key, creating a fresh random one the first time. Returns whether it was just created.
pub fn open_or_create(ks: &dyn KeyStore) -> Result<(Unlocked, bool)> {
    if let Some(k) = ks.get()? { return Ok((Unlocked::from_dek(Key(k)), false)); }
    let k: [u8; 32] = crypto::random(); ks.set(&k)?;
    if ks.get()? != Some(k) { return Err(Error::Format("the key could not be stored reliably; refusing to create data that could become unreadable".into())); }
    Ok((Unlocked::from_dek(Key(k)), true))
}

// ---------------------------------------------------------------- portable key export (for moving or restoring on another computer)
#[derive(Serialize, Deserialize)] struct Bundle { version: u32, kdf: KdfParams, salt: String, wrapped: String }
const AAD: &[u8] = b"veyra/key-export/v1";
/// Wraps the data key with a key derived (Argon2id) from `passphrase`. The result is safe to store next to a backup; without the passphrase it is useless.
pub fn export_key(u: &Unlocked, passphrase: &str, kdf: KdfParams) -> Result<String> {
    if passphrase.chars().count() < 10 { return Err(Error::Format("the export passphrase must have at least 10 characters".into())); }
    let salt: [u8; 16] = crypto::random(); let kek = crypto::derive_kek(passphrase, &salt, kdf)?; let wrapped = crypto::seal(&kek, AAD, &u.dek_bytes())?;
    serde_json::to_string(&Bundle { version: 1, kdf, salt: B64.encode(salt), wrapped: B64.encode(wrapped) }).map_err(|e| Error::Format(e.to_string()))
}
pub fn import_key(bundle: &str, passphrase: &str) -> Result<Unlocked> {
    let b: Bundle = serde_json::from_str(bundle).map_err(|e| Error::Format(format!("key export unreadable: {e}")))?; if b.version != 1 { return Err(Error::Format("unsupported key export version".into())); }
    let salt = B64.decode(&b.salt).map_err(|_| Error::Format("bad salt".into()))?; let wrapped = B64.decode(&b.wrapped).map_err(|_| Error::Format("bad key data".into()))?;
    let kek = crypto::derive_kek(passphrase, &salt, b.kdf)?; let raw = crypto::open(&kek, AAD, &wrapped).map_err(|_| Error::WrongSecret)?;
    Ok(Unlocked::from_dek(Key(raw.try_into().map_err(|_| Error::Format("bad key length".into()))?)))
}
/// Stores an imported key as THIS computer's key (used when restoring a backup on a new machine).
pub fn adopt(ks: &dyn KeyStore, u: &Unlocked) -> Result<()> { ks.set(&u.dek_bytes()) }

#[cfg(test)]
mod tests {
    use super::*;
    fn tmp() -> PathBuf { let d = std::env::temp_dir().join(format!("veyra-sk-{}-{}", std::process::id(), crypto::hex(&crypto::random::<4>()))); std::fs::create_dir_all(&d).unwrap(); d }
    #[test] fn key_is_created_once_then_reused_without_any_password() {
        let d = tmp(); let ks = FileKeyStore { path: d.join("k") };
        let (u1, created) = open_or_create(&ks).unwrap(); assert!(created); let (u2, created2) = open_or_create(&ks).unwrap(); assert!(!created2);
        assert_eq!(crypto::hex(&u1.db_key().0), crypto::hex(&u2.db_key().0)); assert_ne!(crypto::hex(&u1.db_key().0), crypto::hex(&u1.file_key().0), "subkeys differ");
        #[cfg(unix)] { use std::os::unix::fs::PermissionsExt; assert_eq!(std::fs::metadata(d.join("k")).unwrap().permissions().mode() & 0o777, 0o600); }
    }
    #[test] fn export_import_round_trip_needs_the_right_passphrase_and_never_contains_the_key_in_clear() {
        let d = tmp(); let ks = FileKeyStore { path: d.join("k") }; let (u, _) = open_or_create(&ks).unwrap();
        let b = export_key(&u, "a long passphrase!", KdfParams::TEST).unwrap(); assert!(!b.contains(&crypto::hex(&u.dek_bytes())));
        assert!(matches!(import_key(&b, "wrong passphrase!!"), Err(Error::WrongSecret)));
        let u2 = import_key(&b, "a long passphrase!").unwrap(); assert_eq!(u2.dek_bytes(), u.dek_bytes());
        assert!(export_key(&u, "short", KdfParams::TEST).is_err());
    }
    #[test] fn adopting_an_imported_key_makes_it_this_computers_key() {
        let (a, b) = (tmp(), tmp()); let ksa = FileKeyStore { path: a.join("k") }; let ksb = FileKeyStore { path: b.join("k") };
        let (u, _) = open_or_create(&ksa).unwrap(); let bundle = export_key(&u, "another long one", KdfParams::TEST).unwrap();
        let imported = import_key(&bundle, "another long one").unwrap(); adopt(&ksb, &imported).unwrap(); let (u2, created) = open_or_create(&ksb).unwrap(); assert!(!created); assert_eq!(u2.dek_bytes(), u.dek_bytes());
    }
    #[test] fn corrupt_key_files_are_reported_not_replaced() { let d = tmp(); std::fs::write(d.join("k"), "zz").unwrap(); assert!(open_or_create(&FileKeyStore { path: d.join("k") }).is_err()); }
}
