use std::sync::Arc;

use reqwest::{StatusCode, header::RETRY_AFTER};
use tokio::sync::Mutex;

use crate::{
    RepositoryError, ServiceRegistry,
    dtos::{
        error::{PublicError, ToPublicError},
        page::{Page, PaginationParams},
        songs::{CooldownInfo, SearchParams, SongDto, SongWithCooldownInfo},
    },
    entities,
    liquidsoap::{LiquidsoapClient, LiquidsoapError, QueueItem},
    repositories::{
        AlwaysCloneableConnection, BaseRepository,
        favourite_songs::{CreateFavouriteSongDto, FavouriteSongRepositoryExt},
        song_history::SongHistoryRepositoryExt,
        song_requests::CreateSongRequestDto,
        songs::{SongFilter, SongRepositoryExt},
    },
    services::{cooldowns::CooldownService, users::UserService},
};

use super::{
    UserId,
    cooldowns::{
        CooldownServiceError, GlobalCooldown, UserCooldown, global::SongCooldown,
        user::SongRequestCooldown,
    },
    users::UserServiceError,
};

#[derive(thiserror::Error, Debug)]
pub enum SongServiceError {
    #[error(
        "This song has been requested too recently. You can request another song in {0} seconds."
    )]
    SongCooldown(i64),

    #[error("You have recently requested a song. You can request another song in {0} seconds.")]
    UserCooldown(i64),

    #[error("This song is not available")]
    SongAlreadyPlaying(i64),

    #[error("Song not found")]
    SongNotFound(String),

    #[error("There is no song that is currently playing")]
    NoCurrentlyPlayingSong,

    #[error(transparent)]
    Repository(#[from] RepositoryError),
    #[error(transparent)]
    User(#[from] UserServiceError),
    #[error(transparent)]
    Cooldown(#[from] CooldownServiceError),
    #[error(transparent)]
    Liquidsoap(#[from] LiquidsoapError),
    #[error(transparent)]
    SerdeJson(#[from] serde_json::Error),
}

impl ToPublicError for SongServiceError {
    fn as_public(&self) -> Option<crate::dtos::error::PublicError> {
        match self {
            SongServiceError::SongCooldown(seconds) => Some(
                PublicError::with_owned("song-cooldown", self.to_string(), StatusCode::CONFLICT)
                    .with_header(RETRY_AFTER, seconds.to_string()),
            ),
            SongServiceError::UserCooldown(seconds) => Some(
                PublicError::with_owned(
                    "user-cooldown",
                    self.to_string(),
                    StatusCode::TOO_MANY_REQUESTS,
                )
                .with_header(RETRY_AFTER, seconds.to_string()),
            ),
            SongServiceError::SongAlreadyPlaying(seconds) => Some(
                PublicError::with_owned(
                    "song-already-playing",
                    self.to_string(),
                    StatusCode::CONFLICT,
                )
                .with_header(RETRY_AFTER, seconds.to_string()),
            ),
            SongServiceError::SongNotFound(file_hash) => Some(
                PublicError::with_owned("song-not-found", self.to_string(), StatusCode::NOT_FOUND)
                    .with_header("file_hash", file_hash),
            ),
            SongServiceError::NoCurrentlyPlayingSong => Some(PublicError::with_owned(
                "no-currently-playing-song",
                self.to_string(),
                StatusCode::NOT_FOUND,
            )),
            SongServiceError::User(e) => e.as_public(),
            SongServiceError::Cooldown(e) => e.as_public(),
            _ => None,
        }
    }
}

pub struct SongService {
    // repositories
    song_repo: BaseRepository<entities::songs::Entity>,
    user_repo: BaseRepository<entities::users::Entity>,
    song_request_repo: BaseRepository<entities::song_requests::Entity>,
    song_history_repo: BaseRepository<entities::played_songs::Entity>,
    favourite_song_repo: BaseRepository<entities::favourite_songs::Entity>,
    tags_repo: BaseRepository<entities::song_tags::Entity>,

    // services
    user_service: Arc<UserService>,
    cooldown_service: Arc<CooldownService>,

    liquidsoap_client: Arc<Mutex<dyn LiquidsoapClient>>,
}

impl SongService {
    pub fn new(
        db: &AlwaysCloneableConnection,
        registry: &ServiceRegistry,
        liquidsoap_client: Arc<Mutex<dyn LiquidsoapClient>>,
    ) -> Self {
        Self {
            song_repo: BaseRepository::new(db),
            user_repo: BaseRepository::new(db),
            song_request_repo: BaseRepository::new(db),
            song_history_repo: BaseRepository::new(db),
            favourite_song_repo: BaseRepository::new(db),
            tags_repo: BaseRepository::new(db),
            user_service: registry.user_service(),
            cooldown_service: registry.cooldown_service(),
            liquidsoap_client,
        }
    }

    pub async fn request_song(
        &self,
        user_id: UserId,
        file_hash: &str,
    ) -> Result<SongWithCooldownInfo, SongServiceError> {
        // ensure user
        self.user_service.get_user(user_id).await?;
        self.user_service.update_user_activity(user_id).await?;

        // check if user has requested a song recently
        let cooldown = SongRequestCooldown;
        if cooldown
            .on_cooldown(&self.cooldown_service, user_id)
            .await?
        {
            // user has requested a song recently
            let expires_at = cooldown
                .get(&self.cooldown_service, user_id)
                .await?
                .expect("Cooldown should be set");
            let now = chrono::Utc::now().naive_utc();
            return Err(SongServiceError::UserCooldown(
                (expires_at - now).num_seconds(),
            ));
        }

        // check if the song requested actually exists
        let Some(song) = self.song_repo.find_by_hash(file_hash).await? else {
            return Err(SongServiceError::SongNotFound(file_hash.to_string()));
        };

        // check if the song requested is already playing
        let currently_playing = self.song_history_repo.get_playing().await?;
        if let Some(playing) = currently_playing.filter(|p| p.song_id == file_hash) {
            let now = chrono::Utc::now().naive_utc();
            let elapsed = now - playing.played_at;
            let left = song.duration.round() as i64 - elapsed.num_seconds();
            return Err(SongServiceError::SongAlreadyPlaying(left));
        }

        // check if the song has been requested recently
        let song_cooldown = SongCooldown::new(file_hash, song.duration);
        if song_cooldown.on_cooldown(&self.cooldown_service).await? {
            let expires_at = song_cooldown
                .get(&self.cooldown_service)
                .await?
                .expect("Cooldown should be set");
            let now = chrono::Utc::now().naive_utc();
            return Err(SongServiceError::SongCooldown(
                (expires_at - now).num_seconds(),
            ));
        }

        {
            let mut guard = self.liquidsoap_client.lock().await;
            guard
                .command_with_reconnect(&format!("srq.push {}", &song.file_path))
                .await?;
        }

        self.song_request_repo
            .add(CreateSongRequestDto {
                song_id: file_hash.to_string(),
                user_id: user_id.into(),
            })
            .await?;

        let now = chrono::Utc::now();
        let cooldown_info = CooldownInfo {
            user_cooldown_expires_at: now + cooldown.duration(),
            song_cooldown_expires_at: now + song_cooldown.duration(),
        };
        let song = SongWithCooldownInfo {
            song: song.into(),
            cooldown_info,
        };

        cooldown.set(&self.cooldown_service, user_id).await?;
        song_cooldown.set(&self.cooldown_service).await?;

        Ok(song)
    }

    pub async fn get_request_queue(&self) -> Result<Vec<SongDto>, SongServiceError> {
        let mut client = self.liquidsoap_client.lock().await;
        let response = client.command("song_request_queue").await?;

        let queue: Vec<QueueItem> = serde_json::from_str(&response)?;
        let mut songs = Vec::new();

        for item in queue {
            let song = self.song_repo.read(&item.filename).await?;
            let Some(song) = song else {
                continue;
            };
            songs.push(song.into());
        }

        Ok(songs)
    }

    pub async fn get_song_history(
        &self,
        pagination: &PaginationParams,
    ) -> Result<Page<SongDto>, SongServiceError> {
        self.song_repo
            .find_recently_played(pagination)
            .await
            .map_err(Into::into)
            .map(|page| page.map(|song| song.into()))
    }

    pub async fn search_song(
        &self,
        params: &SearchParams,
        pagination: &PaginationParams,
    ) -> Result<Page<SongDto>, SongServiceError> {
        let mut filter = SongFilter::new()
            .search(&params.query)
            .page(pagination.page)
            .page_size(pagination.page_size);
        if let Some(artist) = &params.artist {
            filter = filter.artist(artist);
        }

        if let Some(album) = &params.album {
            filter = filter.album(album);
        }

        if let Some(title) = &params.title {
            filter = filter.title(title);
        }

        self.song_repo
            .browse(filter)
            .await
            .map_err(Into::into)
            .map(|page| page.map(|song| song.into()))
    }

    pub async fn search_favourite_songs(
        &self,
        user_id: UserId,
        params: &SearchParams,
        pagination: &PaginationParams,
    ) -> Result<Page<SongDto>, SongServiceError> {
        let mut filter = SongFilter::new()
            .search(&params.query)
            .page(pagination.page)
            .page_size(pagination.page_size)
            .favourited_by(user_id.into());
        if let Some(artist) = &params.artist {
            filter = filter.artist(artist);
        }

        if let Some(album) = &params.album {
            filter = filter.album(album);
        }

        if let Some(title) = &params.title {
            filter = filter.title(title);
        }

        self.song_repo
            .browse(filter)
            .await
            .map_err(Into::into)
            .map(|page| page.map(|song| song.into()))
    }

    pub async fn get_currently_playing_song(&self) -> Result<SongDto, SongServiceError> {
        let played = self.song_history_repo.get_playing().await?;

        if let Some(playing) = played {
            self.song_repo
                .find_by_hash(&playing.song_id)
                .await
                .map_err(Into::into)
                .and_then(|song| {
                    song.ok_or(SongServiceError::SongNotFound(playing.song_id.clone()))
                        .map(|song| song.into())
                })
        } else {
            Err(SongServiceError::NoCurrentlyPlayingSong)
        }
    }

    pub async fn mark_song_as_favourite(
        &self,
        user_id: UserId,
        song_id: &str,
    ) -> Result<(), SongServiceError> {
        if self
            .favourite_song_repo
            .is_favourited(user_id.into(), song_id)
            .await?
        {
            return Ok(());
        }

        self.favourite_song_repo
            .add(CreateFavouriteSongDto {
                user_id: user_id.into(),
                song_id: song_id.to_string(),
            })
            .await
            .map_err(Into::into)
            .map(|_| ())
    }

    pub async fn unmark_song_as_favourite(
        &self,
        user_id: UserId,
        song_id: &str,
    ) -> Result<(), SongServiceError> {
        if !self
            .favourite_song_repo
            .is_favourited(user_id.into(), song_id)
            .await?
        {
            return Ok(());
        }

        self.favourite_song_repo
            .remove_by_user_song(user_id.into(), song_id)
            .await
            .map_err(Into::into)
    }

    pub async fn mark_currently_playing_song_as_favourite(
        &self,
        user_id: UserId,
    ) -> Result<(), SongServiceError> {
        let song = self.get_currently_playing_song().await?;

        if self
            .favourite_song_repo
            .is_favourited(user_id.into(), &song.id)
            .await?
        {
            return Ok(());
        }

        self.favourite_song_repo
            .add(CreateFavouriteSongDto {
                user_id: user_id.into(),
                song_id: song.id,
            })
            .await
            .map_err(Into::into)
            .map(|_| ())
    }
}
