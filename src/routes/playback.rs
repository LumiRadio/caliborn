//! Inbound `/playback/played` endpoint, called by Liquidsoap when a song
//! starts playing. Authenticated via the `X-Liquidsoap-Token` header against
//! the `liquidsoap_ingest_token` configured at startup (sourced from
//! `CALIBORN_LIQUIDSOAP_TOKEN`).

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

fn check_token(headers: &HeaderMap, configured: &str) -> Result<(), ApiError> {
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
    check_token(&headers, &state.liquidsoap_ingest_token)?;

    let played_at = state
        .service_registry
        .stream_service()
        .record_played(&req.file_path, req.title, req.artist, req.album)
        .await?;

    // Best-effort: if this play came from a song request, debounce-push the
    // requester's linked-role metadata to Discord. Spawned so /played
    // returns even if Discord is slow.
    let registry = state.service_registry.clone();
    let file_path = req.file_path.clone();
    tokio::spawn(async move {
        if let Err(e) = maybe_push_linked_roles(&registry, &file_path).await {
            tracing::debug!(error = ?e, "linked-role debounce push failed");
        }
    });

    Ok(PlayedResponse { played_at })
}

async fn maybe_push_linked_roles(
    registry: &crate::services::ServiceRegistry,
    file_path: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use crate::{entities, services::discord_linked_roles};
    use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder};

    let db = registry.db_handle();
    let last_req = entities::song_requests::Entity::find()
        .filter(entities::song_requests::Column::SongId.eq(file_path))
        .order_by_desc(entities::song_requests::Column::CreatedAt)
        .one(&*db)
        .await?;
    let Some(req) = last_req else {
        return Ok(());
    };
    let user_id = req.user_id;

    if !discord_linked_roles::should_push_after(&db, user_id, chrono::Duration::hours(1)).await? {
        return Ok(());
    }

    let access_token = registry.token_store().valid_access_token(user_id).await?;
    let metadata = discord_linked_roles::build_metadata(&db, user_id).await?;
    registry
        .linked_roles_service()
        .push_for_user(user_id, &access_token, &metadata)
        .await?;
    Ok(())
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/played", post(played))
}
