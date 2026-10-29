pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("session expired, sign in again")]
    SessionExpired,
    #[error("wrong email or password")]
    BadCredentials,
    #[error("{0}")]
    BadAddress(String),
    #[error("{0}")]
    BadConfig(String),
    #[error("cloud request failed ({status}): {message}")]
    Http {
        status: u16,
        code: String,
        message: String,
        /// Seconds from a `Retry-After` header, when the server sent one.
        retry_after: Option<u64>,
    },
    #[error("cloud unreachable: {0}")]
    Network(String),
    /// A failure on this machine while a pass was running, such as the database write that records a pushed hash.
    #[error("{0}")]
    Local(String),
    /// The caller stopped a pass at a batch boundary, such as a disconnect.
    #[error("the push was stopped")]
    Cancelled,
}

impl Error {
    /// A stable identifier the UI and `yogurt ctl` switch on. The wording of the message may change, this may not.
    pub fn code(&self) -> &'static str {
        match self {
            Error::SessionExpired => "session_expired",
            Error::BadCredentials => "bad_credentials",
            Error::BadAddress(_) => "bad_address",
            Error::BadConfig(_) => "bad_config",
            Error::Network(_) => "network",
            Error::Local(_) => "local_error",
            Error::Cancelled => "cancelled",
            Error::Http { status, code, .. } => match (status, code.as_str()) {
                (412, _) | (_, "PT412") => "passcode_not_set",
                (404, "PT404") => "device_removed",
                // PostgREST's "no such function": the Cloud was deployed before this RPC existed.
                (404, "PGRST202") => "cloud_outdated",
                // 42501 also comes from the one-hour push window, so only connect maps it to `not_owner`.
                _ => "cloud_error",
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn http(status: u16, code: &str) -> Error {
        Error::Http {
            status,
            code: code.into(),
            message: String::new(),
            retry_after: None,
        }
    }

    #[test]
    fn codes_are_stable() {
        assert_eq!(Error::SessionExpired.code(), "session_expired");
        assert_eq!(Error::BadCredentials.code(), "bad_credentials");
        assert_eq!(Error::BadAddress("x".into()).code(), "bad_address");
        assert_eq!(Error::BadConfig("x".into()).code(), "bad_config");
        assert_eq!(Error::Network("x".into()).code(), "network");
        assert_eq!(http(412, "PT412").code(), "passcode_not_set");
        assert_eq!(http(404, "PT404").code(), "device_removed");
        assert_eq!(http(404, "PGRST202").code(), "cloud_outdated");
        assert_eq!(http(403, "42501").code(), "cloud_error");
        assert_eq!(http(404, "").code(), "cloud_error");
        assert_eq!(http(500, "XX000").code(), "cloud_error");
    }
}
