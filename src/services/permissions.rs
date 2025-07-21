use crate::{RepositoryError, dtos::error::ToPublicError};

#[derive(thiserror::Error, Debug)]
pub enum PermissionServiceError {
    #[error(transparent)]
    Repository(#[from] RepositoryError),
}

impl ToPublicError for PermissionServiceError {
    fn as_public(&self) -> Option<crate::dtos::error::PublicError> {
        match self {
            Self::Repository(_) => None,
        }
    }
}

#[async_trait::async_trait]
pub trait PermissionService: Send + Sync + 'static {}
