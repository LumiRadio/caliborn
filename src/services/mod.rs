use std::{
    fmt::Display,
    sync::{Arc, OnceLock},
};

use hmac::Hmac;
use sha2::Sha256;
use tokio::sync::Mutex;

use crate::{
    DiscordOAuthClient,
    liquidsoap::LiquidsoapClient,
    repositories::AlwaysCloneableConnection,
    services::{
        admin::AdminCrudService, auth::AuthService, cans::CansService, cooldowns::CooldownService,
        economy::EconomyService, minigames::MinigameService, songs::SongService,
        users::UserService,
    },
};

pub mod admin;
pub mod auth;
pub mod cans;
pub mod cooldowns;
pub mod economy;
pub mod minigames;
pub mod songs;
pub mod users;

pub struct CachedService<T> {
    cell: Arc<OnceLock<Arc<T>>>,
}

impl<T> CachedService<T> {
    pub fn new() -> Self {
        Self {
            cell: Arc::new(OnceLock::new()),
        }
    }

    pub fn get_or_init<F>(&self, init: F) -> Arc<T>
    where
        F: FnOnce() -> T,
    {
        self.cell.get_or_init(|| Arc::new(init())).clone()
    }
}

impl<T> Clone for CachedService<T> {
    fn clone(&self) -> Self {
        Self {
            cell: Arc::clone(&self.cell),
        }
    }
}

#[derive(Clone, Copy)]
pub struct UserId(u64);

impl From<UserId> for u64 {
    fn from(id: UserId) -> Self {
        id.0
    }
}

impl From<u64> for UserId {
    fn from(id: u64) -> Self {
        UserId(id)
    }
}

impl From<UserId> for i64 {
    fn from(id: UserId) -> Self {
        id.0 as i64
    }
}

impl From<i64> for UserId {
    fn from(id: i64) -> Self {
        UserId(id as u64)
    }
}

impl Display for UserId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Clone)]
pub struct ServiceRegistry {
    jwt_secret: Hmac<Sha256>,
    hmac_secret: Hmac<Sha256>,
    oauth_client: DiscordOAuthClient,
    liquidsoap_client: Arc<Mutex<dyn LiquidsoapClient>>,
    db: AlwaysCloneableConnection,

    // services
    admin_service: CachedService<AdminCrudService>,
    auth_service: CachedService<AuthService>,
    economy_service: CachedService<EconomyService>,
    user_service: CachedService<UserService>,
    can_service: CachedService<CansService>,
    cooldown_service: CachedService<CooldownService>,
    song_service: CachedService<SongService>,
    minigame_service: CachedService<MinigameService>,
}

impl ServiceRegistry {
    pub fn new(
        db: AlwaysCloneableConnection,
        jwt_secret: Hmac<Sha256>,
        hmac_secret: Hmac<Sha256>,
        oauth_client: DiscordOAuthClient,
        liquidsoap_client: Arc<Mutex<dyn LiquidsoapClient>>,
    ) -> Self {
        Self {
            db,
            jwt_secret,
            hmac_secret,
            oauth_client,
            admin_service: CachedService::new(),
            auth_service: CachedService::new(),
            economy_service: CachedService::new(),
            user_service: CachedService::new(),
            can_service: CachedService::new(),
            cooldown_service: CachedService::new(),
            song_service: CachedService::new(),
            minigame_service: CachedService::new(),
            liquidsoap_client,
        }
    }

    pub fn admin_service(&self) -> Arc<AdminCrudService> {
        self.admin_service
            .get_or_init(|| AdminCrudService::new(&self.db))
    }

    pub fn auth_service(&self) -> Arc<AuthService> {
        self.auth_service.get_or_init(|| {
            AuthService::new(
                &self.db,
                self.oauth_client.clone(),
                self.jwt_secret.clone(),
                self.hmac_secret.clone(),
            )
        })
    }

    pub fn economy_service(&self) -> Arc<EconomyService> {
        self.economy_service
            .get_or_init(|| EconomyService::new(&self.db, self))
    }

    pub fn user_service(&self) -> Arc<UserService> {
        self.user_service
            .get_or_init(|| UserService::new(&self.db, self))
    }

    pub fn can_service(&self) -> Arc<CansService> {
        self.can_service
            .get_or_init(|| CansService::new(&self.db, self))
    }

    pub fn cooldown_service(&self) -> Arc<CooldownService> {
        self.cooldown_service
            .get_or_init(|| CooldownService::new(&self.db))
    }

    pub fn song_service(&self) -> Arc<SongService> {
        self.song_service
            .get_or_init(|| SongService::new(&self.db, self, self.liquidsoap_client.clone()))
    }

    pub fn minigame_service(&self) -> Arc<MinigameService> {
        self.minigame_service
            .get_or_init(|| MinigameService::new(&self.db, self))
    }
}
