use axum::{
    Router,
    extract::State,
    routing::{get, post},
};

use crate::{
    AppState,
    dtos::{
        Json,
        economy::{PayRequest, PayResponse},
        error::{CalibornResult, ErrorResponse},
        users::UserDto,
    },
    services::auth::{AuthenticatedUser, authenticate},
};

#[utoipa::path(
    get,
    path = "/user/me",
    responses(
        (status = 200, description = "User was successfully retrieved", body = UserDto),
        (status = 401, description = "An authorization error occurred (e.g. invalid access token)", body = ErrorResponse, example = json!({"message": "Invalid access token", "error": "Unauthorized"})),
        (status = 500, description = "An internal server error occurred", body = ErrorResponse, example = json!({"message": "Internal server error", "error": "Internal Server Error"}))
    )
)]
#[axum::debug_handler]
pub async fn me(
    AuthenticatedUser(actor): AuthenticatedUser,
    State(state): State<AppState>,
) -> CalibornResult<UserDto> {
    let user_id = actor.user_id();

    let user_service = state.service_registry.user_service();
    let user = user_service.get_user(user_id).await?;
    user_service.update_user_activity(user_id).await?;

    Ok(user)
}

#[utoipa::path(
    post,
    path = "/user/me/pay",
    request_body = PayRequest,
    responses(
        (status = 200, description = "Boonbucks transferred", body = PayResponse),
        (status = 401, description = "An authorization error occurred", body = ErrorResponse),
        (status = 422, description = "Invalid amount, self-transfer, insufficient funds, or recipient not found", body = ErrorResponse),
        (status = 500, description = "An internal server error occurred", body = ErrorResponse)
    ),
    security(
        ("user_jwt" = []),
        ("user_api_key" = [])
    )
)]
#[axum::debug_handler]
pub async fn pay(
    AuthenticatedUser(actor): AuthenticatedUser,
    State(state): State<AppState>,
    Json(payload): Json<PayRequest>,
) -> CalibornResult<PayResponse> {
    let economy = state.service_registry.economy_service();
    let (sender_balance, recipient_balance) = economy
        .pay(actor.user_id(), payload.recipient_id.into(), payload.amount)
        .await?;

    Ok(PayResponse {
        amount: payload.amount,
        sender_balance,
        recipient_balance,
    })
}

pub fn routes(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/me", get(me))
        .route("/me/pay", post(pay))
        .layer(axum::middleware::from_fn_with_state(state, authenticate))
}
