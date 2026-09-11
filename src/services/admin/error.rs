use reqwest::StatusCode;
use serde::Serialize;
use utoipa::ToSchema;

use crate::{
    RepositoryError,
    dtos::error::{PublicError, ToPublicError},
};

#[derive(thiserror::Error, Debug)]
pub enum AdminError {
    #[error("unknown resource `{0}`")]
    UnknownResource(String),
    #[error("invalid id `{0}`: {1}")]
    BadId(String, String),
    #[error("invalid payload: {0}")]
    Deserialize(String),
    #[error("resource `{0}` with id `{1}` not found")]
    NotFound(String, String),
    #[error(transparent)]
    Repository(#[from] crate::repositories::RepositoryError),
}

impl ToPublicError for AdminError {
    fn as_public(&self) -> Option<crate::dtos::error::PublicError> {
        match self {
            Self::UnknownResource(_) => Some(PublicError::with_owned(
                "resource-not-found",
                self.to_string(),
                StatusCode::NOT_FOUND,
            )),
            Self::BadId(_, _) => Some(PublicError::with_owned(
                "bad-id",
                self.to_string(),
                StatusCode::BAD_REQUEST,
            )),
            Self::Deserialize(_) => Some(PublicError::with_owned(
                "bad-payload",
                self.to_string(),
                StatusCode::BAD_REQUEST,
            )),
            Self::NotFound(_, _) => Some(PublicError::with_owned(
                "record-not-found",
                self.to_string(),
                StatusCode::NOT_FOUND,
            )),
            Self::Repository(RepositoryError::UpdateNotFound(_, _)) => {
                Some(PublicError::with_owned(
                    "record-not-found",
                    self.to_string(),
                    StatusCode::NOT_FOUND,
                ))
            }
            _ => None,
        }
    }
}
