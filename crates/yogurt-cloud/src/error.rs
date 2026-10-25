pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("passphrase must be at least 12 characters")]
    PassphraseTooShort,
    #[error("invalid public key")]
    InvalidPublicKey,
    #[error("invalid private key")]
    InvalidPrivateKey,
    #[error("malformed ciphertext")]
    MalformedCiphertext,
    #[error("unsupported kdf")]
    UnsupportedKdf,
    #[error("decryption failed")]
    Decrypt,
    #[error("encryption failed")]
    Encrypt,
    #[error("invalid base64: {0}")]
    Base64(#[from] base64::DecodeError),
    #[error("invalid json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid link code: {0}")]
    LinkCode(&'static str),
}
