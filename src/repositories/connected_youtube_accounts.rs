//! DTOs for YouTube channels linked to a Discord user.
//!
//! Rows are normally written by the OAuth login flow
//! ([`crate::services::auth::AuthService::login_user`]); the admin CRUD surface
//! exists so an operator can repair a bad link without a psql session.

use crate::{entities, generate_dtos};

generate_dtos!(
    entities::connected_youtube_accounts::Entity,
    CreateConnectedYoutubeAccountDto {
        user_id: i64,
        youtube_channel_id: String,
        youtube_channel_name: String,
    },
    UpdateConnectedYoutubeAccountDto {
        user_id: Option<i64>,
        youtube_channel_id: Option<String>,
        youtube_channel_name: Option<String>,
    }
);
