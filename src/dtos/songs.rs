use axum::response::IntoResponse;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::{dtos::json, entities, vectorizer::to_tsvector};

#[derive(Deserialize, Serialize, ToSchema)]
pub struct SongDto {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration: f64,
    pub bitrate: i32,
}

impl IntoResponse for SongDto {
    fn into_response(self) -> axum::response::Response {
        json(self).into_response()
    }
}

impl From<entities::songs::Model> for SongDto {
    fn from(value: entities::songs::Model) -> Self {
        Self {
            id: value.file_hash,
            title: value.title,
            artist: value.artist,
            album: value.album,
            duration: value.duration,
            bitrate: value.bitrate,
        }
    }
}

#[derive(Serialize)]
pub struct SongListDto(Vec<SongDto>);

impl From<Vec<SongDto>> for SongListDto {
    fn from(value: Vec<SongDto>) -> Self {
        Self(value)
    }
}

impl IntoResponse for SongListDto {
    fn into_response(self) -> axum::response::Response {
        json(self).into_response()
    }
}

#[derive(Serialize, ToSchema)]
pub struct PlayInfo {
    pub play_count: i32,
    pub request_count: i32,
    pub last_played_at: DateTime<Utc>,
    pub last_requested_at: DateTime<Utc>,
    pub on_cooldown: bool,
    pub cooldown_expires_at: DateTime<Utc>,
}

#[derive(Serialize, ToSchema)]
pub struct SongWithPlayInfo {
    #[serde(flatten)]
    pub song: SongDto,
    #[serde(flatten)]
    pub play_info: PlayInfo,
}

impl SongWithPlayInfo {
    pub fn new(song: SongDto, play_info: PlayInfo) -> Self {
        Self { song, play_info }
    }
}

#[derive(Serialize, ToSchema)]
pub struct CooldownInfo {
    pub user_cooldown_expires_at: DateTime<Utc>,
    pub song_cooldown_expires_at: DateTime<Utc>,
}

#[derive(Serialize, ToSchema)]
pub struct SongWithCooldownInfo {
    #[serde(flatten)]
    pub song: SongDto,
    #[serde(flatten)]
    pub cooldown_info: CooldownInfo,
}

impl SongWithCooldownInfo {
    pub fn new(song: SongDto, cooldown_info: CooldownInfo) -> Self {
        Self {
            song,
            cooldown_info,
        }
    }
}

impl IntoResponse for SongWithCooldownInfo {
    fn into_response(self) -> axum::response::Response {
        json(self).into_response()
    }
}

#[derive(Deserialize, ToSchema)]
pub struct SongRequest {
    pub file_hash: String,
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct SearchParams {
    pub query: String,

    #[serde(rename = "filter[artist]")]
    pub artist: Option<String>,

    #[serde(rename = "filter[album]")]
    pub album: Option<String>,

    #[serde(rename = "filter[title]")]
    pub title: Option<String>,
}

impl SearchParams {
    pub fn as_ts_query(&self) -> String {
        let query = to_tsvector(&self.query);

        query
            .lexemes
            .iter()
            .map(|lexeme| format!("{}:*", lexeme.term))
            .collect::<Vec<_>>()
            .join(" & ")
    }
}
