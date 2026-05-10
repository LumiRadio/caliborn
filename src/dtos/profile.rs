//! User profile DTO returned by `GET /user/me/profile` and `GET /user/{id}/profile`.

use axum::response::IntoResponse;
use chrono::NaiveDateTime;
use serde::Serialize;
use utoipa::ToSchema;

use super::json;

#[derive(Serialize, ToSchema, Debug, Clone)]
#[schema(
    description = "A linked YouTube channel.",
    examples(json!({"channel_id": "UCxxxx", "channel_name": "Cool Channel"}))
)]
pub struct ConnectedYoutubeAccountDto {
    pub channel_id: String,
    pub channel_name: String,
}

#[derive(Serialize, ToSchema, Debug, Clone)]
#[schema(
    description = "Aggregated profile view of a user.",
    examples(json!({
        "id": 675674657_i64,
        "username": "Caliborn",
        "boonbucks": 1234,
        "watched_time": 36000,
        "listening_hours": 10,
        "position_by_hours": 3,
        "position_by_boonbucks": 7,
        "cans_added": 25,
        "connected_youtube_accounts": [],
        "role": "user",
        "permissions": ["use_minigames", "use_web_chat", "use_bot"],
        "created_at": "2023-01-01T00:00:00",
        "updated_at": "2023-01-01T00:00:00"
    }))
)]
pub struct ProfileDto {
    pub id: i64,
    pub username: Option<String>,
    pub boonbucks: i32,
    /// Watched (listening) time in seconds.
    pub watched_time: i64,
    /// `watched_time / 3600`, exposed for convenience.
    pub listening_hours: i32,
    /// 1-based rank by `watched_time` descending.
    pub position_by_hours: i64,
    /// 1-based rank by `boonbucks` descending.
    pub position_by_boonbucks: i64,
    /// Number of cans this user has placed in Can Town.
    pub cans_added: i64,
    pub connected_youtube_accounts: Vec<ConnectedYoutubeAccountDto>,
    /// The user's primary role name.
    pub role: String,
    /// Effective permissions (role grants + per-user grants − per-user revokes).
    pub permissions: Vec<String>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

impl IntoResponse for ProfileDto {
    fn into_response(self) -> axum::response::Response {
        json(self).into_response()
    }
}
