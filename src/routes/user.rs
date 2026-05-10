use axum::{
    Router,
    extract::{Path, State},
    routing::{get, post},
};

use crate::{
    AppState,
    dtos::{
        Json,
        economy::{PayRequest, PayResponse},
        error::{CalibornResult, ErrorResponse},
        profile::ProfileDto,
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

#[utoipa::path(
    post,
    path = "/user/me/sync-linked-role",
    responses(
        (status = 204, description = "Linked-role metadata pushed to Discord"),
        (status = 401, description = "Auth or stored token missing", body = ErrorResponse),
        (status = 502, description = "Discord rejected the push or refresh", body = ErrorResponse),
        (status = 500, body = ErrorResponse)
    ),
    security(("user_jwt" = []), ("user_api_key" = []))
)]
#[axum::debug_handler]
pub async fn sync_linked_role(
    AuthenticatedUser(actor): AuthenticatedUser,
    State(state): State<AppState>,
) -> CalibornResult<axum::http::StatusCode> {
    use crate::dtos::error::{ApiError, PublicError};
    use reqwest::StatusCode;

    let registry = &state.service_registry;
    let user_id: i64 = actor.user_id().into();

    let access_token = registry
        .token_store()
        .valid_access_token(user_id)
        .await
        .map_err(|e| {
            ApiError::Public(PublicError::with_owned(
                "no-stored-tokens",
                format!("Stored Discord tokens missing or unrefreshable: {e}"),
                StatusCode::UNAUTHORIZED,
            ))
        })?;

    let metadata =
        crate::services::discord_linked_roles::build_metadata(&registry.db_handle(), user_id)
            .await
            .map_err(|e| ApiError::Internal(anyhow::anyhow!("failed to build metadata: {e}")))?;

    registry
        .linked_roles_service()
        .push_for_user(user_id, &access_token, &metadata)
        .await
        .map_err(|e| {
            ApiError::Public(PublicError::with_owned(
                "discord-push-failed",
                format!("Discord rejected the push: {e}"),
                StatusCode::BAD_GATEWAY,
            ))
        })?;

    Ok(axum::http::StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get,
    path = "/user/me/profile",
    responses(
        (status = 200, description = "Aggregated profile for the authenticated user", body = ProfileDto),
        (status = 401, body = ErrorResponse),
        (status = 500, body = ErrorResponse)
    ),
    security(("user_jwt" = []), ("user_api_key" = []))
)]
#[axum::debug_handler]
pub async fn my_profile(
    AuthenticatedUser(actor): AuthenticatedUser,
    State(state): State<AppState>,
) -> CalibornResult<ProfileDto> {
    let profile = state
        .service_registry
        .user_service()
        .get_profile(actor.user_id())
        .await?;
    Ok(profile)
}

#[utoipa::path(
    get,
    path = "/user/{id}/profile",
    responses(
        (status = 200, description = "Aggregated profile for the requested user", body = ProfileDto),
        (status = 401, body = ErrorResponse),
        (status = 500, body = ErrorResponse)
    ),
    security(("user_jwt" = []), ("user_api_key" = []))
)]
#[axum::debug_handler]
pub async fn user_profile(
    _actor: AuthenticatedUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> CalibornResult<ProfileDto> {
    let profile = state
        .service_registry
        .user_service()
        .get_profile(id.into())
        .await?;
    Ok(profile)
}

pub fn routes(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/me", get(me))
        .route("/me/profile", get(my_profile))
        .route("/me/pay", post(pay))
        .route("/me/sync-linked-role", post(sync_linked_role))
        .route("/{id}/profile", get(user_profile))
        .layer(axum::middleware::from_fn_with_state(state, authenticate))
}
