use std::{collections::HashMap, str::FromStr};

use sea_orm::{DatabaseConnection, EntityTrait, IntoActiveModel, PrimaryKeyTrait};
use serde::{Serialize, de::DeserializeOwned};

use crate::{
    repositories::{
        ApplyUpdates, NoDto,
        cans::{CreateCanDto, UpdateCanDto},
        connected_youtube_accounts::{
            CreateConnectedYoutubeAccountDto, UpdateConnectedYoutubeAccountDto,
        },
        cooldowns::{CreateCooldownDto, UpdateCooldownDto},
        favourite_songs::{CreateFavouriteSongDto, UpdateFavouriteSongDto},
        roles::{CreateRoleDto, UpdateRoleDto},
        server_channel_config::{CreateServerChannelConfigDto, UpdateServerChannelConfigDto},
        server_config::{CreateServerConfigDto, UpdateServerConfigDto},
        slcb::{
            CreateSlcbCurrencyDto, CreateSlcbRankDto, UpdateSlcbCurrencyDto, UpdateSlcbRankDto,
        },
        song_history::CreatePlayedSongDto,
        song_requests::CreateSongRequestDto,
        songs::{CreateSongDto, UpdateSongDto},
        tags::{CreateTagDto, UpdateTagDto},
        users::{CreateUserDto, UpdateUserDto},
    },
    services::admin::{
        error::AdminError,
        resource::{AdminResource, EntityResource},
    },
};

pub struct AdminRegistry {
    db: DatabaseConnection,
    resources: HashMap<String, Box<dyn AdminResource>>,
}

impl AdminRegistry {
    pub fn new(db: &DatabaseConnection) -> Self {
        Self {
            db: db.clone(),
            resources: HashMap::new(),
        }
    }

    pub fn register<E, C, U>(mut self, name: &str) -> Self
    where
        E: EntityTrait + Default + Send + Sync,
        E::Model: IntoActiveModel<E::ActiveModel> + Serialize + Send + Sync,
        E::ActiveModel: Send,
        E::Column: sea_orm::Iterable,
        <E::PrimaryKey as PrimaryKeyTrait>::ValueType: FromStr + ToString + Clone + Send,
        <<E::PrimaryKey as PrimaryKeyTrait>::ValueType as FromStr>::Err: std::fmt::Display,
        C: IntoActiveModel<E::ActiveModel> + DeserializeOwned + Send + Sync,
        U: ApplyUpdates<E::ActiveModel> + DeserializeOwned + Send + Sync,
        C: 'static,
        U: 'static,
    {
        self.resources.insert(
            name.to_string(),
            Box::new(EntityResource::<E, C, U>::new(&self.db, name)),
        );

        self
    }

    pub fn get(&self, name: &str) -> Result<&dyn AdminResource, AdminError> {
        self.resources
            .get(name)
            .map(|b| b.as_ref())
            .ok_or_else(|| AdminError::UnknownResource(name.to_string()))
    }

    pub fn names(&self) -> Vec<&str> {
        self.resources.keys().map(String::as_str).collect()
    }
}

pub fn build_registry(db: &DatabaseConnection) -> AdminRegistry {
    use crate::entities::*;
    AdminRegistry::new(db)
        .register::<users::Entity, CreateUserDto, UpdateUserDto>("users")
        .register::<cans::Entity, CreateCanDto, UpdateCanDto>("cans")
        .register::<cooldown::Entity, CreateCooldownDto, UpdateCooldownDto>("cooldowns")
        .register::<favourite_songs::Entity, CreateFavouriteSongDto, UpdateFavouriteSongDto>(
            "favourite_songs",
        )
        .register::<roles::Entity, CreateRoleDto, UpdateRoleDto>("roles")
        .register::<server_channel_config::Entity, CreateServerChannelConfigDto, UpdateServerChannelConfigDto>("server_channel_config")
        .register::<server_config::Entity, CreateServerConfigDto, UpdateServerConfigDto>("server_config")
        .register::<song_requests::Entity, CreateSongRequestDto, NoDto>("song_requests")
        .register::<songs::Entity, CreateSongDto, UpdateSongDto>("songs")
        .register::<song_tags::Entity, CreateTagDto, UpdateTagDto>("song_tags")
        .register::<played_songs::Entity, CreatePlayedSongDto, NoDto>("played_songs")
        .register::<slcb_currency::Entity, CreateSlcbCurrencyDto, UpdateSlcbCurrencyDto>(
            "slcb_currency",
        )
        .register::<slcb_rank::Entity, CreateSlcbRankDto, UpdateSlcbRankDto>("slcb_rank")
        .register::<
            connected_youtube_accounts::Entity,
            CreateConnectedYoutubeAccountDto,
            UpdateConnectedYoutubeAccountDto,
        >("connected_youtube_accounts")
}
