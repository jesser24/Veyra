//! Encrypted local storage. Nothing here touches the network.
//! Two key modes. System mode (default, no password): random DEK in the OS credential store, see system_key.rs. Legacy password mode below.
//! Key hierarchy (password mode): master password --Argon2id--> KEK --wraps--> DEK (random, 32 bytes) --HKDF--> database key and attachment key.
//! A recovery key wraps the same DEK, so changing the password never re-encrypts data. Lose both and the data is unrecoverable.
pub mod backup;
pub mod crypto;
pub mod db;
pub mod vault;
pub mod system_key;

#[derive(Debug)]
pub enum Error { Crypto(String), WrongSecret, Io(std::io::Error), Db(String), Format(String), NotFound(String), Integrity(String) }
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Crypto(s) | Error::Db(s) | Error::Format(s) | Error::NotFound(s) | Error::Integrity(s) => write!(f, "{s}"),
            Error::WrongSecret => write!(f, "wrong password or recovery key"),
            Error::Io(e) => write!(f, "{e}"),
        }
    }
}
impl std::error::Error for Error {}
impl From<std::io::Error> for Error { fn from(e: std::io::Error) -> Self { Error::Io(e) } }
impl From<rusqlite::Error> for Error { fn from(e: rusqlite::Error) -> Self { Error::Db(e.to_string()) } }
pub type Result<T> = std::result::Result<T, Error>;
