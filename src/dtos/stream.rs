//! Stream control + playback ingest data transfer objects.

use axum::response::IntoResponse;
use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::json;

#[derive(Deserialize, ToSchema)]
#[schema(examples(json!({"level": 0.6})))]
pub struct VolumeRequest {
    /// Level in `[0.0, 1.0]`.
    #[schema(example = 0.6, minimum = 0.0, maximum = 1.0)]
    pub level: f32,
}

#[derive(Deserialize, ToSchema)]
#[schema(examples(json!({"file_path": "/music/example.flac", "priority": false})))]
pub struct QueuePushRequest {
    pub file_path: String,
    /// If true, push to the priority queue (`prioq`) instead of the standard
    /// request queue (`srq`).
    #[serde(default)]
    pub priority: bool,
}

#[derive(Deserialize, ToSchema)]
#[schema(examples(json!({"command": "request.skip"})))]
pub struct RawCommandRequest {
    pub command: String,
}

#[derive(Serialize, ToSchema)]
#[schema(examples(json!({"response": "Done.\n"})))]
pub struct LiquidsoapResponseDto {
    pub response: String,
}

impl IntoResponse for LiquidsoapResponseDto {
    fn into_response(self) -> axum::response::Response {
        json(self).into_response()
    }
}

#[derive(Deserialize, ToSchema)]
#[schema(examples(json!({
    "file_path": "/music/example.flac",
    "title": "Example Song",
    "artist": "Example Artist",
    "album": "Example Album"
})))]
pub struct PlayedRequest {
    pub file_path: String,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
}

#[derive(Serialize, ToSchema)]
#[schema(examples(json!({"played_at": "2026-05-10T12:34:56"})))]
pub struct PlayedResponse {
    pub played_at: NaiveDateTime,
}

impl IntoResponse for PlayedResponse {
    fn into_response(self) -> axum::response::Response {
        json(self).into_response()
    }
}
