//! Minigame-related data transfer objects.

use axum::response::IntoResponse;
use serde::Deserialize;
use utoipa::ToSchema;

use super::json;

pub use crate::services::minigames::dice::RollResult as DiceRollResult;
pub use crate::services::minigames::slots::{ReelSymbol, SlotSymbol, SpinResult};

/// Request body for `POST /minigames/slots/spin`.
#[derive(Deserialize, ToSchema)]
#[schema(
    description = "Slot machine spin request",
    examples(json!({"bet": 5}))
)]
pub struct SlotsSpinRequest {
    /// Wager in boonbucks. Must be between 1 and 10 inclusive.
    #[schema(example = 5, minimum = 1, maximum = 10)]
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
