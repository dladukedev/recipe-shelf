use super::*;

pub(super) fn server_error(error: impl Into<anyhow::Error>) -> topcoat::Error {
    eprintln!("request failed: {:#}", error.into());
    internal_server_error(std::io::Error::other("request failed")).into()
}

/// Expected form failures may be shown beside the form. Unexpected failures
/// must remain server errors rather than masquerading as validation failures.
pub(super) fn accepted<T>(result: anyhow::Result<T>) -> topcoat::Result<bool> {
    use crate::domain_error::DomainError;
    match result {
        Ok(_) => Ok(true),
        Err(error) => match error.downcast_ref::<DomainError>() {
            Some(
                DomainError::InvalidInput(_) | DomainError::Conflict(_) | DomainError::NotFound(_),
            ) => Ok(false),
            Some(DomainError::Forbidden(_)) => Err(forbidden().into()),
            Some(DomainError::InvalidCredentials) => Err(unauthorized().into()),
            None => Err(server_error(error)),
        },
    }
}
