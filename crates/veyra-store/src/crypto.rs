use crate::{Error, Result};
use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use hkdf::Hkdf;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use zeroize::{Zeroize, ZeroizeOnDrop};

#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct Key(pub [u8; 32]);
impl std::fmt::Debug for Key { fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "Key(..)") } }

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct KdfParams { pub m_kib: u32, pub t: u32, pub p: u32 }
impl KdfParams {
    /// Production default: 64 MiB, 3 passes, 1 lane (Argon2id). Tune on the target machines before release.
    pub const DEFAULT: KdfParams = KdfParams { m_kib: 64 * 1024, t: 3, p: 1 };
    /// Cheap parameters for unit tests only.
    pub const TEST: KdfParams = KdfParams { m_kib: 8, t: 1, p: 1 };
}

pub fn random<const N: usize>() -> [u8; N] { let mut b = [0u8; N]; getrandom::fill(&mut b).expect("OS random generator unavailable"); b }

pub fn derive_kek(password: &str, salt: &[u8], p: KdfParams) -> Result<Key> {
    let params = Params::new(p.m_kib, p.t, p.p, Some(32)).map_err(|e| Error::Crypto(e.to_string()))?;
    let mut out = [0u8; 32];
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params).hash_password_into(password.as_bytes(), salt, &mut out).map_err(|e| Error::Crypto(e.to_string()))?;
    Ok(Key(out))
}

/// `nonce(24) || ciphertext+tag`. `aad` binds the ciphertext to its purpose/identity (a blob cannot be swapped for another).
pub fn seal(key: &Key, aad: &[u8], plain: &[u8]) -> Result<Vec<u8>> {
    let nonce: [u8; 24] = random();
    let ct = XChaCha20Poly1305::new((&key.0).into()).encrypt(XNonce::from_slice(&nonce), Payload { msg: plain, aad }).map_err(|_| Error::Crypto("encryption failed".into()))?;
    let mut out = nonce.to_vec(); out.extend(ct); Ok(out)
}
pub fn open(key: &Key, aad: &[u8], data: &[u8]) -> Result<Vec<u8>> {
    if data.len() < 24 + 16 { return Err(Error::Format("ciphertext too short".into())); }
    XChaCha20Poly1305::new((&key.0).into()).decrypt(XNonce::from_slice(&data[..24]), Payload { msg: &data[24..], aad }).map_err(|_| Error::WrongSecret)
}
pub fn subkey(dek: &Key, label: &str) -> Key {
    let mut out = [0u8; 32];
    Hkdf::<Sha256>::new(None, &dek.0).expand(label.as_bytes(), &mut out).expect("32 bytes is a valid HKDF length");
    Key(out)
}
pub fn sha256_hex(data: &[u8]) -> String { use sha2::Digest; hex(&Sha256::digest(data)) }
pub fn hex(b: &[u8]) -> String { b.iter().map(|x| format!("{x:02x}")).collect() }
pub fn unhex(s: &str) -> Option<Vec<u8>> { if s.len() % 2 != 0 { return None; } (0..s.len()).step_by(2).map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok()).collect() }
