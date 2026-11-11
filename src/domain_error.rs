use std::fmt;

/// Expected application failures. Storage and task failures retain their original
/// error types, so HTTP handlers can distinguish them from rejected input.
#[derive(Debug)]
pub enum DomainError {
    InvalidCredentials,
    InvalidInput(&'static str),
    Forbidden(&'static str),
    NotFound(&'static str),
    Conflict(&'static str),
}

impl fmt::Display for DomainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidCredentials => f.write_str("invalid username or password"),
            Self::InvalidInput(message)
            | Self::Forbidden(message)
            | Self::NotFound(message)
            | Self::Conflict(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for DomainError {}

pub(crate) fn account_write_error(error: rusqlite::Error) -> anyhow::Error {
    match &error {
        rusqlite::Error::SqliteFailure(code, _)
            if code.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE =>
        {
            DomainError::Conflict("username is already in use").into()
        }
        _ => error.into(),
    }
}
