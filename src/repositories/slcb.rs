//! DTOs for the Streamlabs Chatbot import tables.
//!
//! These tables are **permanent** — they're consulted every time a new user
//! links Discord + YouTube to find their pre-Discord activity — so they're
//! exposed through the generic admin CRUD registry for operator fix-ups.

use crate::{entities, generate_dtos};

generate_dtos!(
    entities::slcb_currency::Entity,
    CreateSlcbCurrencyDto {
        username: String,
        points: i32,
        hours: i32,
        user_id: Option<String>,
    },
    UpdateSlcbCurrencyDto {
        username: Option<String>,
        points: Option<i32>,
        hours: Option<i32>,
        user_id: Option<Option<String>>,
    }
);

generate_dtos!(
    entities::slcb_rank::Entity,
    CreateSlcbRankDto {
        rank_name: String,
        hour_requirement: i32,
        channel_id: Option<String>,
    },
    UpdateSlcbRankDto {
        rank_name: Option<String>,
        hour_requirement: Option<i32>,
        channel_id: Option<Option<String>>,
    }
);
