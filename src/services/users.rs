use std::sync::Arc;

use chrono::{TimeDelta, Utc};
use reqwest::StatusCode;
use shared_constants::permissions::Permission;

use crate::{
    ServiceRegistry,
    dtos::{
        error::{PublicError, ToPublicError},
        profile::{ConnectedYoutubeAccountDto, ProfileDto},
        users::UserDto,
    },
    entities,
    repositories::{
        AlwaysCloneableConnection, BaseRepository, RepositoryError,
        permissions::UserPermissionRepositoryExt,
        users::{CreateUserDto, UpdateUserDto, UserRepositoryExt},
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

    /// Aggregate user profile: balances, listening hours + leaderboard
    /// position, can count, linked YouTube channels, role, effective
    /// permissions.
    ///
    /// Auto-creates the user row if missing (matches [`Self::get_user`]).
    pub async fn get_profile(&self, id: UserId) -> Result<ProfileDto, UserServiceError> {
        use sea_orm::{DbBackend, FromQueryResult, Statement};

        // Ensure the row exists.
        self.create_user(id).await?;

        let user_id: i64 = id.into();
        let (user, channels) = self.user_repo.find_with_channels(user_id).await?.ok_or(
            RepositoryError::UpdateNotFound("users".to_string(), user_id.to_string()),
        )?;
        let cans_added = self.user_repo.can_count(user_id).await? as i64;

        let permissions = self
            .permissions_repo
            .get_effective_permissions(user_id)
            .await?
            .into_iter()
            .map(|p| p.name.to_string())
            .collect::<Vec<_>>();

        // Ranks via 1 + COUNT of strictly-greater rows. Cheap on indexed cols
        // for our user count; revisit with window functions if it gets slow.
        #[derive(FromQueryResult)]
        struct Rank {
            rank: i64,
        }
        let position_by_hours = Rank::find_by_statement(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT 1 + COUNT(*) AS rank FROM users WHERE watched_time > $1",
            [user.watched_time.into()],
        ))
        .one(&self.user_repo.db_handle())
        .await
        .map_err(RepositoryError::from)?
        .map(|r| r.rank)
        .unwrap_or(1);
        let position_by_boonbucks = Rank::find_by_statement(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT 1 + COUNT(*) AS rank FROM users WHERE boonbucks > $1",
            [user.boonbucks.into()],
        ))
        .one(&self.user_repo.db_handle())
        .await
        .map_err(RepositoryError::from)?
        .map(|r| r.rank)
        .unwrap_or(1);

        Ok(ProfileDto {
            id: user.id,
            username: user.username,
            boonbucks: user.boonbucks,
            watched_time: user.watched_time,
            listening_hours: (user.watched_time / 3600) as i32,
            position_by_hours,
            position_by_boonbucks,
            cans_added,
            connected_youtube_accounts: channels
                .into_iter()
                .map(|c| ConnectedYoutubeAccountDto {
                    channel_id: c.youtube_channel_id,
                    channel_name: c.youtube_channel_name,
                })
                .collect(),
            role: user.role,
            permissions,
            created_at: user.created_at,
            updated_at: user.updated_at,
        })
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
