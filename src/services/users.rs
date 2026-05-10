use std::sync::Arc;

use chrono::{TimeDelta, Utc};
use reqwest::StatusCode;
use shared_constants::permissions::Permission;

use crate::{
    ServiceRegistry,
    dtos::{
        error::{PublicError, ToPublicError},
        users::UserDto,
    },
    entities,
    repositories::{
        AlwaysCloneableConnection, BaseRepository, RepositoryError,
        permissions::UserPermissionRepositoryExt,
        users::{CreateUserDto, UpdateUserDto},
    },
    services::cooldowns::CooldownService,
};

use super::{
    UserId,
    cooldowns::{CooldownServiceError, UserCooldown, user::UserActivityCooldown},
};

#[derive(thiserror::Error, Debug)]
pub enum UserServiceError {
    #[error("User does not have permission `{permission}`")]
    PermissionDenied { permission: String },
    #[error("Role `{role}` not found")]
    RoleNotFound { role: String },

    #[error("Error while checking activity cooldown")]
    ActivityCooldownError(#[from] CooldownServiceError),

    #[error(transparent)]
    Repository(#[from] RepositoryError),
}

impl ToPublicError for UserServiceError {
    fn as_public(&self) -> Option<PublicError> {
        match self {
            UserServiceError::ActivityCooldownError(cd) => cd.as_public(),
            UserServiceError::RoleNotFound { .. } => Some(PublicError::with_owned(
                "role-not-found",
                self.to_string(),
                StatusCode::UNPROCESSABLE_ENTITY,
            )),
            UserServiceError::PermissionDenied { .. } => Some(PublicError::with_owned(
                "permission-denied",
                self.to_string(),
                StatusCode::FORBIDDEN,
            )),
            _ => None,
        }
    }
}

pub struct UserService {
    user_repo: BaseRepository<entities::users::Entity>,
    permissions_repo: BaseRepository<entities::user_permissions::Entity>,
    roles_repo: BaseRepository<entities::roles::Entity>,

    cooldown_service: Arc<CooldownService>,
}

impl UserService {
    pub fn new(db: &AlwaysCloneableConnection, registry: &ServiceRegistry) -> Self {
        Self {
            user_repo: BaseRepository::new(db),
            permissions_repo: BaseRepository::new(db),
            roles_repo: BaseRepository::new(db),
            cooldown_service: registry.cooldown_service(),
        }
    }

    /// Creates a new user
    ///
    /// If the user already exists, it will be returned
    pub async fn create_user(&self, id: UserId) -> Result<UserDto, UserServiceError> {
        let user = match self.user_repo.read(Into::<i64>::into(id)).await? {
            Some(user) => user,
            None => self.user_repo.add(CreateUserDto { id: id.into() }).await?,
        };

        Ok(user.into())
    }

    /// Gets a user by their ID
    ///
    /// If the user does not exist, it will be created
    pub async fn get_user(&self, id: UserId) -> Result<UserDto, UserServiceError> {
        self.create_user(id).await
    }

    pub async fn update_user_boonbucks(
        &self,
        id: UserId,
        amount: i32,
    ) -> Result<(), UserServiceError> {
        // ensure user exists
        self.get_user(id).await?;

        self.user_repo
            .edit(
                Into::<i64>::into(id),
                UpdateUserDto {
                    boonbucks: Some(amount),
                    ..Default::default()
                },
            )
            .await?;

        Ok(())
    }

    pub async fn update_user_activity_time(&self, id: UserId) -> Result<(), UserServiceError> {
        let user = self.get_user(id).await?;
        let now = Utc::now().naive_utc();
        let time_diff = if let Some(last_message_sent) = user.last_message_sent {
            now - last_message_sent
        } else {
            TimeDelta::zero()
        };

        self.user_repo
            .edit(
                Into::<i64>::into(id),
                UpdateUserDto {
                    last_message_sent: Some(Some(now)),
                    watched_time: Some(
                        user.watched_time
                            + chrono::Duration::minutes(15).min(time_diff).num_seconds(),
                    ),
                    ..Default::default()
                },
            )
            .await?;

        Ok(())
    }

    pub async fn update_user_activity(&self, id: UserId) -> Result<(), UserServiceError> {
        self.update_user_activity_time(id).await?;
        let user = self.get_user(id).await?;
        let cooldown = UserActivityCooldown;
        if cooldown.on_cooldown(&self.cooldown_service, id).await? {
            return Ok(());
        }

        self.update_user_boonbucks(id, user.boonbucks + 3).await?;

        Ok(())
    }

    pub async fn user_has_permission(
        &self,
        id: UserId,
        permission: Permission,
    ) -> Result<(), UserServiceError> {
        self.get_user(id).await?;
        let permissions = self
            .permissions_repo
            .get_effective_permissions(id.into())
            .await?;
        if !permissions.contains(&permission) {
            return Err(UserServiceError::PermissionDenied {
                permission: permission.name.to_string(),
            });
        }
        Ok(())
    }
}
