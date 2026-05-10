use axum::{Router, extract::State, routing::post};

use crate::{
    AppState,
    dtos::{
        Json,
        error::{CalibornResult, ErrorResponse},
        minigames::{SlotsSpinRequest, SpinResult},
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

pub fn routes(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/slots/spin", post(slots_spin))
        .layer(axum::middleware::from_fn_with_state(state, authenticate))
}
