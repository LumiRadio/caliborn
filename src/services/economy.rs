use std::sync::Arc;

use reqwest::StatusCode;

use crate::{
    ServiceRegistry,
    dtos::error::{PublicError, ToPublicError},
    entities,
    repositories::{
        AlwaysCloneableConnection, BaseRepository, RepositoryError,
        users::{TransferError, UserRepositoryExt},
    },
    services::users::{UserService, UserServiceError},
};

use super::UserId;

#[derive(thiserror::Error, Debug)]
pub enum EconomyServiceError {
    #[error("Sender not found")]
    SenderNotFound,
    #[error("Target user not found")]
    TargetUserNotFound,
    #[error("Cannot pay yourself")]
    SelfTransfer,
    #[error("Invalid amount")]
    InvalidAmount,
    #[error("Insufficient funds")]
    InsufficientFunds,
    #[error(transparent)]
    Repository(#[from] RepositoryError),
    #[error(transparent)]
    UserService(#[from] UserServiceError),
}

impl From<TransferError> for EconomyServiceError {
    fn from(value: TransferError) -> Self {
        match value {
            TransferError::SenderNotFound => EconomyServiceError::SenderNotFound,
            TransferError::RecipientNotFound => EconomyServiceError::TargetUserNotFound,
            TransferError::InsufficientFunds => EconomyServiceError::InsufficientFunds,
            TransferError::Db(e) => EconomyServiceError::Repository(RepositoryError::from(e)),
        }
    }
}

impl ToPublicError for EconomyServiceError {
    fn as_public(&self) -> Option<PublicError> {
        match self {
            EconomyServiceError::SenderNotFound => Some(PublicError::new(
                "sender-not-found",
                "The sending user was not found.",
                StatusCode::UNPROCESSABLE_ENTITY,
            )),
            EconomyServiceError::TargetUserNotFound => Some(PublicError::new(
                "user-not-found",
                "The target user was not found.",
                StatusCode::UNPROCESSABLE_ENTITY,
            )),
            EconomyServiceError::SelfTransfer => Some(PublicError::new(
                "self-transfer",
                "You cannot transfer boonbucks to yourself.",
                StatusCode::UNPROCESSABLE_ENTITY,
            )),
            EconomyServiceError::InvalidAmount => Some(PublicError::new(
                "invalid-amount",
                "The amount provided is invalid.",
                StatusCode::UNPROCESSABLE_ENTITY,
            )),
            EconomyServiceError::InsufficientFunds => Some(PublicError::new(
                "insufficient-funds",
                "The user does not have enough funds.",
                StatusCode::UNPROCESSABLE_ENTITY,
            )),
            EconomyServiceError::UserService(e) => e.as_public(),
            _ => None,
        }
    }
}

pub struct EconomyService {
    user_repo: BaseRepository<entities::users::Entity>,
    user_service: Arc<UserService>,
}

impl EconomyService {
    pub fn new(db: &AlwaysCloneableConnection, registry: &ServiceRegistry) -> Self {
        Self {
            user_repo: BaseRepository::new(db),
            user_service: registry.user_service(),
        }
    }

    pub async fn get_balance(&self, id: UserId) -> Result<i32, EconomyServiceError> {
        let user = self.user_service.get_user(id).await?;

        Ok(user.boonbucks)
    }

    pub async fn add_boonbucks(&self, id: UserId, amount: i32) -> Result<(), EconomyServiceError> {
        let user = self.user_service.get_user(id).await?;

        self.user_service
            .update_user_boonbucks(id, user.boonbucks + amount)
            .await?;

        Ok(())
    }

    pub async fn remove_boonbucks(
        &self,
        id: UserId,
        amount: i32,
    ) -> Result<(), EconomyServiceError> {
        let user = self.user_service.get_user(id).await?;

        self.user_service
            .update_user_boonbucks(id, user.boonbucks - amount)
            .await?;

        Ok(())
    }

    /// Atomically transfers `amount` boonbucks from `from_id` to `to_id`.
    ///
    /// Returns `(sender_balance, recipient_balance)` after the transfer.
    pub async fn transfer_boonbucks(
        &self,
        from_id: UserId,
        to_id: UserId,
        amount: i32,
    ) -> Result<(i32, i32), EconomyServiceError> {
        if amount <= 0 {
            return Err(EconomyServiceError::InvalidAmount);
        }

        let balances = self
            .user_repo
            .transfer_boonbucks(from_id.into(), to_id.into(), amount)
            .await?;

        Ok(balances)
    }

    /// User-initiated payment between two distinct users.
    ///
    /// Wraps [`Self::transfer_boonbucks`] with a self-pay rejection and
    /// ensures the recipient row exists before the transfer.
    pub async fn pay(
        &self,
        from_id: UserId,
        to_id: UserId,
        amount: i32,
    ) -> Result<(i32, i32), EconomyServiceError> {
        if Into::<i64>::into(from_id) == Into::<i64>::into(to_id) {
            return Err(EconomyServiceError::SelfTransfer);
        }

        // Ensure both users exist (creates rows if missing — matches existing service semantics).
        self.user_service.get_user(from_id).await?;
        self.user_service.get_user(to_id).await?;

        self.transfer_boonbucks(from_id, to_id, amount).await
    }
}
