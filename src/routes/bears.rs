use axum::{
    Router,
    extract::State,
    routing::{get, post},
};
use shared_constants::permissions::PERM_USE_MINIGAMES;

use crate::{
    AppState, ServiceRegistry,
    dtos::{
        cans::CanCountDto,
        error::{CalibornResult, ErrorResponse},
    },
    services::{
        auth::{AuthenticatedUser, authenticate},
        cans::CanType,
    },
};

/// Get the current bear count.
///
/// # Returns
///
/// A 200 OK response containing the current bear count.
///
/// # Errors
///
/// * `500 Internal Server Error` - An internal server error occurred
#[utoipa::path(
    get,
    path = "/bears/count",
    responses(
        (status = 200, description = "Current bear count was successfully retrieved", body = CanCountDto),
        (status = 500, description = "An internal server error occurred", body = ErrorResponse, example = json!({"message": "Internal server error", "error": "Internal Server Error"}))
    )
)]
pub async fn get_bear_count(
    AuthenticatedUser(actor): AuthenticatedUser,
    State(registry): State<ServiceRegistry>,
) -> CalibornResult<CanCountDto> {
    let can_service = registry.can_service();
    let user_service = registry.user_service();

    user_service
        .user_has_permission(actor.user_id(), PERM_USE_MINIGAMES)
        .await?;
    user_service.update_user_activity(actor.user_id()).await?;

    let count = can_service.count().await?;
    Ok(CanCountDto { count })
}

/// Add a bear to Bear Town.
///
/// # Returns
///
/// A 200 OK response containing the new bear count.
///
/// # Errors
///
/// * `500 Internal Server Error` - An internal server error occurred
/// * `401 Unauthorized` - An authorization error occurred (e.g. invalid access token)
#[utoipa::path(
    post,
    path = "/bears/add",
    responses(
        (status = 200, description = "Bear was successfully added", body = CanCountDto),
        (status = 500, description = "An internal server error occurred", body = ErrorResponse, example = json!({"message": "Internal server error", "error": "Internal Server Error"})),
        (status = 401, description = "An authorization error occurred (e.g. invalid access token)", body = ErrorResponse, example = json!({"message": "Invalid access token", "error": "Unauthorized"}))
    ),
    security(
        ("user_jwt" = []),
        ("user_api_key" = [])
    )
)]
pub async fn add_bear(
    AuthenticatedUser(actor): AuthenticatedUser,
    State(registry): State<ServiceRegistry>,
) -> CalibornResult<CanCountDto> {
    let can_service = registry.can_service();
    let user_service = registry.user_service();

    user_service
        .user_has_permission(actor.user_id(), PERM_USE_MINIGAMES)
        .await?;
    user_service.update_user_activity(actor.user_id()).await?;

    can_service.add(actor.user_id(), CanType::Bear).await?;
    let count = can_service.count().await?;
    Ok(CanCountDto { count })
}

pub fn routes(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/count", get(get_bear_count))
        .route("/add", post(add_bear))
        .layer(axum::middleware::from_fn_with_state(state, authenticate))
}
