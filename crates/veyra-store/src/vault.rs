use crate::crypto::{self, KdfParams, Key};
use crate::{Error, Result};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

#[derive(Serialize, Deserialize, Clone, Debug)]
struct VaultFile { version: u32, kdf: KdfParams, salt: String, wrapped_by_password: String, wrapped_by_recovery: String }

/// The unlocked data-encryption key. Zeroised when dropped.
pub struct Unlocked { dek: Key }
impl Unlocked {
    /// Wraps an existing data-encryption key (used by the system-protected key mode).
    pub fn from_dek(dek: Key) -> Unlocked { Unlocked { dek } }
    pub(crate) fn dek_bytes(&self) -> [u8; 32] { self.dek.0 }
    pub fn db_key(&self) -> Key { crypto::subkey(&self.dek, "veyra/db/v1") }
    pub fn file_key(&self) -> Key { crypto::subkey(&self.dek, "veyra/files/v1") }
}

const AAD_PW: &[u8] = b"veyra/vault/password/v1";
const AAD_RK: &[u8] = b"veyra/vault/recovery/v1";

/// `XXXX-XXXX-…` (64 hex digits). Shown once at creation; the user must store it offline.
fn format_recovery(k: &[u8; 32]) -> String { crypto::hex(k).as_bytes().chunks(8).map(|c| std::str::from_utf8(c).unwrap()).collect::<Vec<_>>().join("-").to_uppercase() }
fn parse_recovery(s: &str) -> Option<Key> {
    let h: String = s.chars().filter(|c| !c.is_whitespace() && *c != '-').collect::<String>().to_lowercase();
    let b = crypto::unhex(&h)?; let a: [u8; 32] = b.try_into().ok()?; Some(Key(a))
}
fn write(path: &Path, v: &VaultFile) -> Result<()> {
    if let Some(d) = path.parent() { fs::create_dir_all(d)?; }
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_vec_pretty(v).map_err(|e| Error::Format(e.to_string()))?)?;
    fs::rename(tmp, path)?; Ok(())
}
fn read(path: &Path) -> Result<VaultFile> {
    let v: VaultFile = serde_json::from_slice(&fs::read(path)?).map_err(|e| Error::Format(format!("vault file unreadable: {e}")))?;
    if v.version != 1 { return Err(Error::Format(format!("unsupported vault version {}", v.version))); }
    Ok(v)
}
pub fn exists(path: &Path) -> bool { path.is_file() }

/// Create a new vault. Returns the unlocked key and the recovery key (display it once; it is not stored anywhere in clear).
pub fn create(path: &Path, password: &str, kdf: KdfParams) -> Result<(Unlocked, String)> {
    if password.chars().count() < 8 { return Err(Error::Format("password must be at least 8 characters".into())); }
    if path.exists() { return Err(Error::Format("a vault already exists here; refusing to overwrite it".into())); }
    let dek = Key(crypto::random());
    let salt: [u8; 16] = crypto::random();
    let kek = crypto::derive_kek(password, &salt, kdf)?;
    let rk: [u8; 32] = crypto::random();
    let v = VaultFile { version: 1, kdf, salt: B64.encode(salt), wrapped_by_password: B64.encode(crypto::seal(&kek, AAD_PW, &dek.0)?), wrapped_by_recovery: B64.encode(crypto::seal(&Key(rk), AAD_RK, &dek.0)?) };
    write(path, &v)?;
    Ok((Unlocked { dek }, format_recovery(&rk)))
}
fn unwrap_dek(w: &str, key: &Key, aad: &[u8]) -> Result<Unlocked> {
    let raw = B64.decode(w).map_err(|e| Error::Format(e.to_string()))?;
    let d: [u8; 32] = crypto::open(key, aad, &raw)?.try_into().map_err(|_| Error::Format("bad key length".into()))?;
    Ok(Unlocked { dek: Key(d) })
}
pub fn unlock(path: &Path, password: &str) -> Result<Unlocked> {
    let v = read(path)?;
    let salt = B64.decode(&v.salt).map_err(|e| Error::Format(e.to_string()))?;
    unwrap_dek(&v.wrapped_by_password, &crypto::derive_kek(password, &salt, v.kdf)?, AAD_PW)
}
pub fn unlock_with_recovery(path: &Path, recovery: &str) -> Result<Unlocked> {
    let k = parse_recovery(recovery).ok_or(Error::WrongSecret)?;
    unwrap_dek(&read(path)?.wrapped_by_recovery, &k, AAD_RK)
}
/// Re-wrap the DEK under a new password. Data is not re-encrypted. Requires the current password.
pub fn change_password(path: &Path, old: &str, new: &str) -> Result<()> {
    if new.chars().count() < 8 { return Err(Error::Format("password must be at least 8 characters".into())); }
    let u = unlock(path, old)?;
    let mut v = read(path)?;
    let salt: [u8; 16] = crypto::random();
    v.salt = B64.encode(salt);
    v.wrapped_by_password = B64.encode(crypto::seal(&crypto::derive_kek(new, &salt, v.kdf)?, AAD_PW, &u.dek.0)?);
    write(path, &v)
}
/// Forgot the password but have the recovery key: set a new password.
pub fn reset_password_with_recovery(path: &Path, recovery: &str, new: &str) -> Result<()> {
    if new.chars().count() < 8 { return Err(Error::Format("password must be at least 8 characters".into())); }
    let u = unlock_with_recovery(path, recovery)?;
    let mut v = read(path)?;
    let salt: [u8; 16] = crypto::random();
    v.salt = B64.encode(salt);
    v.wrapped_by_password = B64.encode(crypto::seal(&crypto::derive_kek(new, &salt, v.kdf)?, AAD_PW, &u.dek.0)?);
    write(path, &v)
}
