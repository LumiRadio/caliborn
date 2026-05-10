use axum::{Router, extract::State, routing::post};
use shared_constants::permissions::PERM_MANAGE_STREAM;

use crate::{
    AppState,
    dtos::{
        Json,
        error::{CalibornResult, ErrorResponse},
        stream::{LiquidsoapResponseDto, QueuePushRequest, RawCommandRequest, VolumeRequest},
    },
    services::{
        auth::{AuthenticatedUser, authenticate},
        stream::LiquidsoapResponse,
    },
};

fn into_dto(r: LiquidsoapResponse) -> LiquidsoapResponseDto {
    LiquidsoapResponseDto {
        response: r.response,
    }
}

#[utoipa::path(
    post,
    path = "/stream/skip",
    responses(
        (status = 200, description = "Skip command sent", body = LiquidsoapResponseDto),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 403, description = "Lacks manage_stream permission", body = ErrorResponse),
        (status = 502, description = "Liquidsoap socket error", body = ErrorResponse)
    ),
    security(("user_jwt" = []), ("user_api_key" = []))
)]
#[axum::debug_handler]
pub async fn skip(
    AuthenticatedUser(actor): AuthenticatedUser,
    State(state): State<AppState>,
) -> CalibornResult<LiquidsoapResponseDto> {
    state
        .service_registry
        .user_service()
        .user_has_permission(actor.user_id(), PERM_MANAGE_STREAM)
        .await?;
    let r = state.service_registry.stream_service().skip().await?;
    Ok(into_dto(r))
}

#[utoipa::path(
    post,
    path = "/stream/volume",
    request_body = VolumeRequest,
    responses(
        (status = 200, description = "Volume set", body = LiquidsoapResponseDto),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
        (status = 422, body = ErrorResponse),
        (status = 502, body = ErrorResponse)
    ),
    security(("user_jwt" = []), ("user_api_key" = []))
)]
#[axum::debug_handler]
pub async fn volume(
    AuthenticatedUser(actor): AuthenticatedUser,
    State(state): State<AppState>,
    Json(req): Json<VolumeRequest>,
) -> CalibornResult<LiquidsoapResponseDto> {
    state
        .service_registry
        .user_service()
        .user_has_permission(actor.user_id(), PERM_MANAGE_STREAM)
        .await?;
    let r = state
        .service_registry
        .stream_service()
        .set_volume(req.level)
        .await?;
    Ok(into_dto(r))
}

#[utoipa::path(
    post,
    path = "/stream/queue/push",
    request_body = QueuePushRequest,
    responses(
        (status = 200, description = "Pushed onto queue", body = LiquidsoapResponseDto),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
        (status = 422, body = ErrorResponse),
        (status = 502, body = ErrorResponse)
    ),
    security(("user_jwt" = []), ("user_api_key" = []))
)]
#[axum::debug_handler]
pub async fn queue_push(
    AuthenticatedUser(actor): AuthenticatedUser,
    State(state): State<AppState>,
    Json(req): Json<QueuePushRequest>,
) -> CalibornResult<LiquidsoapResponseDto> {
    state
        .service_registry
        .user_service()
        .user_has_permission(actor.user_id(), PERM_MANAGE_STREAM)
        .await?;
    let r = state
        .service_registry
        .stream_service()
        .push_queue(&req.file_path, req.priority)
        .await?;
    Ok(into_dto(r))
}

#[utoipa::path(
    post,
    path = "/stream/playlist/reload",
    responses(
        (status = 200, description = "Playlist reloaded", body = LiquidsoapResponseDto),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
        (status = 502, body = ErrorResponse)
    ),
    security(("user_jwt" = []), ("user_api_key" = []))
)]
#[axum::debug_handler]
pub async fn playlist_reload(
    AuthenticatedUser(actor): AuthenticatedUser,
    State(state): State<AppState>,
) -> CalibornResult<LiquidsoapResponseDto> {
    state
        .service_registry
        .user_service()
        .user_has_permission(actor.user_id(), PERM_MANAGE_STREAM)
        .await?;
    let r = state
        .service_registry
        .stream_service()
        .reload_playlist()
        .await?;
    Ok(into_dto(r))
}

#[utoipa::path(
    post,
    path = "/stream/raw",
    request_body = RawCommandRequest,
    responses(
        (status = 200, description = "Raw response", body = LiquidsoapResponseDto),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
        (status = 422, body = ErrorResponse),
        (status = 502, body = ErrorResponse)
    ),
    security(("user_jwt" = []), ("user_api_key" = []))
)]
#[axum::debug_handler]
pub async fn raw(
    AuthenticatedUser(actor): AuthenticatedUser,
    State(state): State<AppState>,
    Json(req): Json<RawCommandRequest>,
) -> CalibornResult<LiquidsoapResponseDto> {
    state
        .service_registry
        .user_service()
        .user_has_permission(actor.user_id(), PERM_MANAGE_STREAM)
        .await?;
    let r = state
        .service_registry
        .stream_service()
        .raw(&req.command)
        .await?;
    Ok(into_dto(r))
}

pub fn routes(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/skip", post(skip))
        .route("/volume", post(volume))
        .route("/queue/push", post(queue_push))
        .route("/playlist/reload", post(playlist_reload))
        .route("/raw", post(raw))
        .layer(axum::middleware::from_fn_with_state(state, authenticate))
}
