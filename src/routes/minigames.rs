use axum::{Router, extract::State, routing::post};

use crate::{
    AppState,
    dtos::{
        Json,
        error::{CalibornResult, ErrorResponse},
        minigames::{DiceRollResult, PvpChallengeRequest, PvpResult, SlotsSpinRequest, SpinResult},
    },
    services::auth::{AuthenticatedUser, authenticate},
};

#[utoipa::path(
    post,
    path = "/minigames/slots/spin",
    request_body = SlotsSpinRequest,
    responses(
        (status = 200, description = "Spin result", body = SpinResult),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 403, description = "User lacks the use_minigames permission", body = ErrorResponse),
        (status = 422, description = "Bet out of range, insufficient funds, or user not found", body = ErrorResponse),
        (status = 429, description = "On cooldown", body = ErrorResponse)
    ),
    security(
        ("user_jwt" = []),
        ("user_api_key" = [])
    )
)]
#[axum::debug_handler]
pub async fn slots_spin(
    AuthenticatedUser(actor): AuthenticatedUser,
    State(state): State<AppState>,
    Json(payload): Json<SlotsSpinRequest>,
) -> CalibornResult<SpinResult> {
    let minigames = state.service_registry.minigame_service();
    let result = minigames.slots.spin(actor.user_id(), payload.bet).await?;
    Ok(result)
}

#[utoipa::path(
    post,
    path = "/minigames/dice/roll",
    responses(
        (status = 200, description = "Dice roll result", body = DiceRollResult),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 403, description = "User lacks the use_minigames permission", body = ErrorResponse),
        (status = 422, description = "Insufficient funds or user not found", body = ErrorResponse),
        (status = 429, description = "On cooldown", body = ErrorResponse)
    ),
    security(
        ("user_jwt" = []),
        ("user_api_key" = [])
    )
)]
#[axum::debug_handler]
pub async fn dice_roll(
    AuthenticatedUser(actor): AuthenticatedUser,
    State(state): State<AppState>,
) -> CalibornResult<DiceRollResult> {
    let minigames = state.service_registry.minigame_service();
    let result = minigames.dice.roll(actor.user_id()).await?;
    Ok(result)
}

#[utoipa::path(
    post,
    path = "/minigames/pvp",
    request_body = PvpChallengeRequest,
    responses(
        (status = 200, description = "Duel result", body = PvpResult),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 403, description = "User lacks the use_minigames permission", body = ErrorResponse),
        (status = 422, description = "Self-challenge, opponent missing, or insufficient funds", body = ErrorResponse),
        (status = 429, description = "Challenger on cooldown", body = ErrorResponse)
    ),
    security(
        ("user_jwt" = []),
        ("user_api_key" = [])
    )
)]
#[axum::debug_handler]
pub async fn pvp_challenge(
    AuthenticatedUser(actor): AuthenticatedUser,
    State(state): State<AppState>,
    Json(payload): Json<PvpChallengeRequest>,
) -> CalibornResult<PvpResult> {
    let minigames = state.service_registry.minigame_service();
    let result = minigames
        .pvp
        .challenge(actor.user_id(), payload.opponent_id.into())
        .await?;
    Ok(result)
}

pub fn routes(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/slots/spin", post(slots_spin))
        .route("/dice/roll", post(dice_roll))
        .route("/pvp", post(pvp_challenge))
        .layer(axum::middleware::from_fn_with_state(state, authenticate))
}
