//! Minigame-related data transfer objects.

use axum::response::IntoResponse;
use serde::Deserialize;
use utoipa::ToSchema;

use super::json;

pub use crate::services::minigames::dice::RollResult as DiceRollResult;
pub use crate::services::minigames::pvp::PvpResult;
pub use crate::services::minigames::slots::{ReelSymbol, SlotSymbol, SpinResult};

/// Request body for `POST /minigames/pvp`.
#[derive(Deserialize, ToSchema)]
#[schema(
    description = "PvP duel request",
    examples(json!({"opponent_id": 675674657_i64}))
)]
pub struct PvpChallengeRequest {
    /// Discord user ID of the opponent.
    #[schema(examples(675674657_i64))]
    pub opponent_id: i64,
}

/// Request body for `POST /minigames/slots/spin`.
#[derive(Deserialize, ToSchema)]
#[schema(
    description = "Slot machine spin request",
    examples(json!({"bet": 5}))
)]
pub struct SlotsSpinRequest {
    /// Wager in boonbucks. Must be between 1 and 10 inclusive.
    #[schema(examples(5), minimum = 1, maximum = 10)]
    pub bet: i32,
}

impl IntoResponse for SpinResult {
    fn into_response(self) -> axum::response::Response {
        json(self).into_response()
    }
}

impl IntoResponse for DiceRollResult {
    fn into_response(self) -> axum::response::Response {
        json(self).into_response()
    }
}

impl IntoResponse for PvpResult {
    fn into_response(self) -> axum::response::Response {
        json(self).into_response()
    }
}
