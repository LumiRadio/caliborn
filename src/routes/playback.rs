//! Inbound `/playback/played` endpoint, called by Liquidsoap when a song
//! starts playing. Authenticated via the `X-Liquidsoap-Token` header against
//! the `CALIBORN_LIQUIDSOAP_TOKEN` environment variable.

use axum::{
    Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    routing::post,
};
use subtle::ConstantTimeEq;

use crate::{
    AppState,
    dtos::{
        Json,
        error::{ApiError, CalibornResult, ErrorResponse, PublicError},
        stream::{PlayedRequest, PlayedResponse},
    },
};

const TOKEN_HEADER: &str = "x-liquidsoap-token";
const TOKEN_ENV: &str = "CALIBORN_LIQUIDSOAP_TOKEN";

fn check_token(headers: &HeaderMap) -> Result<(), ApiError> {
    let configured = std::env::var(TOKEN_ENV).map_err(|_| {
        ApiError::Internal(anyhow::anyhow!(
            "{TOKEN_ENV} not set; /playback/played is disabled"
        ))
    })?;

    let provided = headers
        .get(TOKEN_HEADER)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| {
            ApiError::Public(PublicError::new(
                "missing-token",
                "Missing X-Liquidsoap-Token header.",
                StatusCode::UNAUTHORIZED,
            ))
        })?;

    if provided.as_bytes().ct_eq(configured.as_bytes()).into() {
        Ok(())
    } else {
        Err(ApiError::Public(PublicError::new(
            "invalid-token",
            "Invalid X-Liquidsoap-Token.",
            StatusCode::UNAUTHORIZED,
        )))
    }
}

#[utoipa::path(
    post,
    path = "/playback/played",
    request_body = PlayedRequest,
    responses(
        (status = 200, description = "Recorded", body = PlayedResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 422, body = ErrorResponse),
        (status = 500, body = ErrorResponse)
    )
)]
#[axum::debug_handler]
pub async fn played(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<PlayedRequest>,
) -> CalibornResult<PlayedResponse> {
    check_token(&headers)?;

    let played_at = state
        .service_registry
        .stream_service()
        .record_played(&req.file_path, req.title, req.artist, req.album)
        .await?;

    Ok(PlayedResponse { played_at })
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/played", post(played))
}
