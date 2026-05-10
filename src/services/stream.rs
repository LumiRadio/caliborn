//! Outbound Liquidsoap control + inbound `/played` ingest.

use std::sync::Arc;

use chrono::{NaiveDateTime, Utc};
use reqwest::StatusCode;
use sea_orm::{ActiveValue, EntityTrait, QueryFilter, prelude::*, sea_query::Expr};
use tokio::sync::Mutex;

use crate::{
    RepositoryError,
    dtos::error::{PublicError, ToPublicError},
    entities,
    liquidsoap::{LiquidsoapClient, LiquidsoapError},
    realtime::{Broadcaster, Event},
    repositories::AlwaysCloneableConnection,
};

/// Liquidsoap command names. Centralized so they can be retuned without
/// hunting through call sites.
mod cmds {
    pub const SKIP: &str = "request.skip";
    pub const PLAYLIST_RELOAD: &str = "playlist.m3u.reload";
}

#[derive(thiserror::Error, Debug)]
pub enum StreamServiceError {
    #[error("Invalid volume level (must be between 0.0 and 1.0)")]
    InvalidVolume,
    #[error("Empty file path")]
    EmptyFilePath,
    #[error("Empty raw command")]
    EmptyRawCommand,

    #[error(transparent)]
    Liquidsoap(#[from] LiquidsoapError),
    #[error(transparent)]
    Repository(#[from] RepositoryError),
    #[error(transparent)]
    Db(#[from] sea_orm::DbErr),
}

impl ToPublicError for StreamServiceError {
    fn as_public(&self) -> Option<PublicError> {
        match self {
            StreamServiceError::InvalidVolume => Some(PublicError::new(
                "invalid-volume",
                "Volume must be between 0.0 and 1.0.",
                StatusCode::UNPROCESSABLE_ENTITY,
            )),
            StreamServiceError::EmptyFilePath => Some(PublicError::new(
                "empty-file-path",
                "File path is required.",
                StatusCode::UNPROCESSABLE_ENTITY,
            )),
            StreamServiceError::EmptyRawCommand => Some(PublicError::new(
                "empty-command",
                "Raw command must not be empty.",
                StatusCode::UNPROCESSABLE_ENTITY,
            )),
            StreamServiceError::Liquidsoap(_) => Some(PublicError::new(
                "liquidsoap-error",
                "Liquidsoap socket error.",
                StatusCode::BAD_GATEWAY,
            )),
            _ => None,
        }
    }
}

/// Result of a Liquidsoap command — the raw response from the socket.
#[derive(Debug, Clone)]
pub struct LiquidsoapResponse {
    pub response: String,
}

pub struct StreamService {
    db: AlwaysCloneableConnection,
    liquidsoap_client: Arc<Mutex<dyn LiquidsoapClient>>,
    broadcaster: Broadcaster,
}

impl StreamService {
    pub fn new(
        db: &AlwaysCloneableConnection,
        liquidsoap_client: Arc<Mutex<dyn LiquidsoapClient>>,
        broadcaster: Broadcaster,
    ) -> Self {
        Self {
            db: db.clone(),
            liquidsoap_client,
            broadcaster,
        }
    }

    async fn send(&self, cmd: &str) -> Result<LiquidsoapResponse, StreamServiceError> {
        let mut guard = self.liquidsoap_client.lock().await;
        let response = guard.command_with_reconnect(cmd).await?;
        Ok(LiquidsoapResponse { response })
    }

    pub async fn skip(&self) -> Result<LiquidsoapResponse, StreamServiceError> {
        self.send(cmds::SKIP).await
    }

    pub async fn set_volume(&self, level: f32) -> Result<LiquidsoapResponse, StreamServiceError> {
        if !(0.0..=1.0).contains(&level) {
            return Err(StreamServiceError::InvalidVolume);
        }
        // Matches byers' `var.set radio_volume = <level>` shape; the actual
        // Liquidsoap script must expose a `radio_volume` interactive variable.
        self.send(&format!("var.set radio_volume = {}", level))
            .await
    }

    pub async fn push_queue(
        &self,
        file_path: &str,
        priority: bool,
    ) -> Result<LiquidsoapResponse, StreamServiceError> {
        if file_path.is_empty() {
            return Err(StreamServiceError::EmptyFilePath);
        }
        let queue = if priority { "prioq" } else { "srq" };
        let response = self.send(&format!("{}.push {}", queue, file_path)).await?;
        self.broadcaster.send(Event::QueueUpdated);
        Ok(response)
    }

    pub async fn reload_playlist(&self) -> Result<LiquidsoapResponse, StreamServiceError> {
        self.send(cmds::PLAYLIST_RELOAD).await
    }

    pub async fn raw(&self, command: &str) -> Result<LiquidsoapResponse, StreamServiceError> {
        let trimmed = command.trim();
        if trimmed.is_empty() {
            return Err(StreamServiceError::EmptyRawCommand);
        }
        self.send(trimmed).await
    }

    /// Record that `file_path` was just played.
    ///
    /// Inserts a row into `played_songs`, increments `songs.played` (best-effort
    /// — silently no-ops if the file is not in the library), and publishes a
    /// `NowPlaying` event on the broadcaster.
    pub async fn record_played(
        &self,
        file_path: &str,
        title: Option<String>,
        artist: Option<String>,
        album: Option<String>,
    ) -> Result<NaiveDateTime, StreamServiceError> {
        let played_at = Utc::now().naive_utc();

        entities::played_songs::Entity::insert(entities::played_songs::ActiveModel {
            song_id: ActiveValue::set(file_path.to_string()),
            played_at: ActiveValue::set(played_at),
            ..Default::default()
        })
        .exec(&*self.db)
        .await?;

        entities::songs::Entity::update_many()
            .col_expr(
                entities::songs::Column::Played,
                Expr::col(entities::songs::Column::Played).add(1),
            )
            .filter(entities::songs::Column::FilePath.eq(file_path))
            .exec(&*self.db)
            .await?;

        self.broadcaster.send(Event::NowPlaying {
            file_path: file_path.to_string(),
            title,
            artist,
            album,
            played_at,
        });

        Ok(played_at)
    }
}
