pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("session expired, sign in again")]
    SessionExpired,
    #[error("wrong email or password")]
    BadCredentials,
    #[error("cloud request failed ({status} {code}): {message}")]
    Http {
        status: u16,
        code: String,
        message: String,
    },
    #[error("cloud unreachable: {0}")]
    Network(String),
}
