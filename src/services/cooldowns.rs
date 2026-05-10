use std::fmt::Display;

use chrono::{DateTime, NaiveDateTime, Utc};

use crate::{
    RepositoryError,
    dtos::error::{PublicError, ToPublicError},
    entities,
    repositories::{
        AlwaysCloneableConnection, BaseRepository,
        cooldowns::{CooldownFilter, CooldownScope, CreateCooldownDto},
    },
};

use super::UserId;

#[derive(Debug, thiserror::Error)]
pub enum CooldownServiceError {
    #[error("Global cooldown `{0}` already exists")]
    GlobalCooldownAlreadyExists(String),
    #[error("User cooldown `{0}` already exists")]
    UserCooldownAlreadyExists(String),

    #[error(transparent)]
    Repository(#[from] RepositoryError),
}

impl ToPublicError for CooldownServiceError {
    fn as_public(&self) -> Option<PublicError> {
        None
    }
}

pub struct CooldownService {
    cooldown_repository: BaseRepository<entities::cooldown::Entity>,
}

impl CooldownService {
    pub fn new(db: &AlwaysCloneableConnection) -> Self {
        Self {
            cooldown_repository: BaseRepository::new(db),
        }
    }

    pub async fn set_global_cooldown(
        &self,
        key: &str,
        duration: chrono::Duration,
    ) -> Result<(), CooldownServiceError> {
        let maybe_cooldown = self
            .cooldown_repository
            .browse(
                CooldownFilter::new()
                    .scope(CooldownScope::Global)
                    .key(key.to_string()),
            )
            .await?
            .items
            .into_iter()
            .next();
        if maybe_cooldown.is_some() {
            return Err(CooldownServiceError::GlobalCooldownAlreadyExists(
                key.to_string(),
            ));
        }

        let now = chrono::Utc::now();
        self.cooldown_repository
            .add(CreateCooldownDto {
                scope: CooldownScope::Global.to_string(),
                key: key.to_string(),
                expires_at: (now + duration).naive_utc(),
                user_id: None,
            })
            .await
            .map_err(|e| CooldownServiceError::from(e))?;

        Ok(())
    }

    pub async fn set_user_cooldown(
        &self,
        key: &str,
        user_id: UserId,
        duration: chrono::Duration,
    ) -> Result<(), CooldownServiceError> {
        if self
            .cooldown_repository
            .browse(
                CooldownFilter::new()
                    .scope(CooldownScope::User)
                    .key(key.to_string())
                    .user_id(user_id.into()),
            )
            .await?
            .items
            .first()
            .is_some()
        {
            return Err(CooldownServiceError::UserCooldownAlreadyExists(
                key.to_string(),
            ));
        }

        let now = chrono::Utc::now();
        self.cooldown_repository
            .add(CreateCooldownDto {
                scope: CooldownScope::User.to_string(),
                user_id: Some(user_id.into()),
                key: key.to_string(),
                expires_at: (now + duration).naive_utc(),
            })
            .await
            .map_err(|e| CooldownServiceError::from(e))?;

        Ok(())
    }

    pub async fn get_user_cooldown(
        &self,
        key: &str,
        user_id: UserId,
    ) -> Result<Option<NaiveDateTime>, CooldownServiceError> {
        let cooldown = self
            .cooldown_repository
            .read_by(
                CooldownFilter::new()
                    .scope(CooldownScope::User)
                    .key(key.to_string())
                    .user_id(user_id.into()),
            )
            .await?;
        let expires_at = cooldown.map(|m| m.expires_at);

        Ok(expires_at)
    }

    pub async fn get_global_cooldown(
        &self,
        key: &str,
    ) -> Result<Option<NaiveDateTime>, CooldownServiceError> {
        let cooldown = self
            .cooldown_repository
            .read_by(
                CooldownFilter::new()
                    .scope(CooldownScope::Global)
                    .key(key.to_string()),
            )
            .await?;
        let expires_at = cooldown.map(|m| m.expires_at);

        Ok(expires_at)
    }

    pub async fn is_on_user_cooldown(
        &self,
        key: &str,
        user_id: UserId,
    ) -> Result<bool, CooldownServiceError> {
        let cooldown = self
            .cooldown_repository
            .read_by(
                CooldownFilter::new()
                    .key(key.to_string())
                    .scope(CooldownScope::User)
                    .user_id(user_id.into()),
            )
            .await?;
        let expires_at = cooldown
            .map(|m| m.expires_at)
            .unwrap_or(DateTime::<Utc>::MIN_UTC.naive_utc());

        Ok(expires_at > chrono::Utc::now().naive_utc())
    }

    pub async fn is_on_global_cooldown(&self, key: &str) -> Result<bool, CooldownServiceError> {
        let cooldown = self
            .cooldown_repository
            .read_by(
                CooldownFilter::new()
                    .key(key.to_string())
                    .scope(CooldownScope::Global),
            )
            .await?;
        let expires_at = cooldown
            .map(|m| m.expires_at)
            .unwrap_or(DateTime::<Utc>::MIN_UTC.naive_utc());

        Ok(expires_at > chrono::Utc::now().naive_utc())
    }
}

#[allow(async_fn_in_trait)]
pub trait GlobalCooldown: Display {
    fn duration(&self) -> chrono::Duration;

    async fn set(&self, service: &CooldownService) -> Result<(), CooldownServiceError> {
        service
            .set_global_cooldown(&self.to_string(), self.duration())
            .await
    }

    async fn get(
        &self,
        service: &CooldownService,
    ) -> Result<Option<NaiveDateTime>, CooldownServiceError> {
        service.get_global_cooldown(&self.to_string()).await
    }

    async fn on_cooldown(&self, service: &CooldownService) -> Result<bool, CooldownServiceError> {
        service.is_on_global_cooldown(&self.to_string()).await
    }
}

#[allow(async_fn_in_trait)]
pub trait UserCooldown: Display {
    fn duration(&self) -> chrono::Duration;

    async fn set(
        &self,
        service: &CooldownService,
        user_id: UserId,
    ) -> Result<(), CooldownServiceError> {
        service
            .set_user_cooldown(&self.to_string(), user_id, self.duration())
            .await
    }

    async fn get(
        &self,
        service: &CooldownService,
        user_id: UserId,
    ) -> Result<Option<NaiveDateTime>, CooldownServiceError> {
        service.get_user_cooldown(&self.to_string(), user_id).await
    }

    async fn on_cooldown(
        &self,
        service: &CooldownService,
        user_id: UserId,
    ) -> Result<bool, CooldownServiceError> {
        service
            .is_on_user_cooldown(&self.to_string(), user_id)
            .await
    }
}

pub mod user {
    use std::fmt::Display;

    use super::UserCooldown;

    macro_rules! impl_user_cooldown {
        ($self_:ident, $cooldown:ty = $key:expr, $duration:expr) => {
            impl Display for $cooldown {
                fn fmt(&$self_, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    write!(f, "{}", $key)
                }
            }

            impl UserCooldown for $cooldown {
                fn duration(&$self_) -> chrono::Duration {
                    $duration
                }
            }
        };

        ($cooldown:ty = $key:expr, $duration:expr) => {
            impl Display for $cooldown {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    write!(f, "{}", $key)
                }
            }

            impl UserCooldown for $cooldown {
                fn duration(&self) -> chrono::Duration {
                    $duration
                }
            }
        };
    }

    pub struct RollDiceCooldown;
    pub struct SlotCooldown;
    pub struct PvPCooldown;
    pub struct SongRequestCooldown;
    pub struct UserActivityCooldown;

    impl_user_cooldown!(
        RollDiceCooldown = "minigames_roll_dice",
        chrono::Duration::minutes(5)
    );
    impl_user_cooldown!(
        SlotCooldown = "minigames_slots",
        chrono::Duration::minutes(5)
    );
    impl_user_cooldown!(PvPCooldown = "minigames_pvp", chrono::Duration::minutes(5));
    impl_user_cooldown!(
        SongRequestCooldown = "song_request",
        chrono::Duration::minutes(90)
    );
    impl_user_cooldown!(
        UserActivityCooldown = "user_activity",
        chrono::Duration::minutes(5)
    );
}

pub mod global {
    use std::fmt::Display;

    use super::GlobalCooldown;

    macro_rules! impl_global_cooldown {
        ($self_:ident, $cooldown:ty = $key:expr, $duration:expr) => {
            impl Display for $cooldown {
                fn fmt(&$self_, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    write!(f, "{}", $key)
                }
            }

            impl GlobalCooldown for $cooldown {
                fn duration(&$self_) -> chrono::Duration {
                    $duration
                }
            }
        };

        ($cooldown:ty = $key:expr, $duration:expr) => {
            impl Display for $cooldown {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    write!(f, "{}", $key)
                }
            }

            impl GlobalCooldown for $cooldown {
                fn duration(&self) -> chrono::Duration {
                    $duration
                }
            }
        };
    }

    pub struct SongCooldown {
        file_hash: String,
        song_length: f64,
    }

    impl SongCooldown {
        pub fn new(file_hash: &str, song_length: f64) -> Self {
            Self {
                file_hash: file_hash.to_string(),
                song_length,
            }
        }
    }

    impl_global_cooldown!(
        self,
        SongCooldown = format!("song_request:{}", &self.file_hash),
        match self.song_length {
            0.0..300.0 => chrono::Duration::seconds(1800),
            300.0..600.0 => chrono::Duration::seconds(3600),
            _ => chrono::Duration::seconds(5413),
        }
    );

    pub struct CanCooldown;

    impl_global_cooldown!(CanCooldown = "can", chrono::Duration::seconds(35));
}
