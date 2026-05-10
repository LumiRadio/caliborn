use axum::{
    Router,
    extract::{Path, State},
    routing::post,
};

use crate::{
    AppState,
    dtos::{
        Json, Query,
        admin::{
            SlcbImportResponse, SlcbLinkQuery, SlcbLinkRequest, SlcbLinkResponse, SlcbMatchResponse,
        },
        error::{CalibornResult, ErrorResponse},
    },
    services::{
        permissions::{ManageSlcb, RequirePermission},
        slcb,
    },
};

#[utoipa::path(
    post,
    path = "/admin/slcb/import",
    request_body = Vec<crate::services::slcb::StreamlabsRecord>,
    responses(
        (status = 200, body = SlcbImportResponse),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
    ),
    security(("user_jwt" = []), ("user_api_key" = []))
)]
#[axum::debug_handler]
pub async fn import(
    _perm: RequirePermission<ManageSlcb>,
    State(state): State<AppState>,
    Json(records): Json<Vec<slcb::StreamlabsRecord>>,
) -> CalibornResult<Json<SlcbImportResponse>> {
    let db = state.service_registry.db_handle();
    let summary = slcb::import_records(&db, &records, false).await?;
    Ok(Json(SlcbImportResponse {
        inserted: summary.inserted,
        updated: summary.updated,
        skipped: summary.skipped,
    }))
}

#[utoipa::path(
    post,
    path = "/admin/slcb/match",
    responses(
        (status = 200, body = SlcbMatchResponse),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
    ),
    security(("user_jwt" = []), ("user_api_key" = []))
)]
#[axum::debug_handler]
pub async fn run_match(
    _perm: RequirePermission<ManageSlcb>,
    State(state): State<AppState>,
) -> CalibornResult<Json<SlcbMatchResponse>> {
    let db = state.service_registry.db_handle();
    let summary = slcb::match_youtube_links(&db).await?;
    Ok(Json(SlcbMatchResponse {
        considered: summary.considered,
        matched: summary.matched,
        already_migrated: summary.already_migrated,
        no_slcb_row: summary.no_slcb_row,
    }))
}

#[utoipa::path(
    post,
    path = "/admin/users/{id}/slcb-link",
    params(SlcbLinkQuery),
    request_body = SlcbLinkRequest,
    responses(
        (status = 200, body = SlcbLinkResponse),
        (status = 404, body = ErrorResponse),
        (status = 409, body = ErrorResponse),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
    ),
    security(("user_jwt" = []), ("user_api_key" = []))
)]
#[axum::debug_handler]
pub async fn force_link(
    _perm: RequirePermission<ManageSlcb>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Query(params): Query<SlcbLinkQuery>,
    Json(payload): Json<SlcbLinkRequest>,
) -> CalibornResult<Json<SlcbLinkResponse>> {
    let db = state.service_registry.db_handle();
    let summary = slcb::link_user_to_slcb_username(
        &db,
        id,
        &payload.slcb_username,
        params.force.unwrap_or(false),
    )
    .await?;
    Ok(Json(SlcbLinkResponse {
        user_id: summary.user_id,
        slcb_username: summary.slcb_username,
        hours_credited: summary.hours_credited,
        points_credited: summary.points_credited,
        watched_time_after: summary.watched_time_after,
        boonbucks_after: summary.boonbucks_after,
    }))
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/slcb/import", post(import))
        .route("/slcb/match", post(run_match))
        .route("/users/{id}/slcb-link", post(force_link))
}
