use axum::{
    Router,
    extract::{Path, State},
    routing::{delete, get},
};
use reqwest::StatusCode;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, Set};

use crate::{
    AppState,
    dtos::{
        Json, Query,
        admin::{
            CooldownBulkClearQuery, CooldownBulkClearResponse, CooldownDto, CooldownListQuery,
            PaginatedResponse, UpsertCooldownRequest,
        },
        error::{ApiError, CalibornResult, ErrorResponse},
    },
    entities,
    services::permissions::{ManageCooldowns, RequirePermission},
};

#[utoipa::path(
    get,
    path = "/admin/cooldowns",
    params(CooldownListQuery),
    responses(
        (status = 200, body = PaginatedResponse<CooldownDto>),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
    ),
    security(("user_jwt" = []), ("user_api_key" = []))
)]
#[axum::debug_handler]
pub async fn list_cooldowns(
    _perm: RequirePermission<ManageCooldowns>,
    State(state): State<AppState>,
    Query(params): Query<CooldownListQuery>,
) -> CalibornResult<Json<PaginatedResponse<CooldownDto>>> {
    let db = state.service_registry.db_handle();
    let page = params.page.unwrap_or(0);
    let page_size = params.page_size.unwrap_or(20).clamp(1, 200);

    let mut query = entities::cooldown::Entity::find();
    if let Some(scope) = params.scope.as_deref() {
        query = query.filter(entities::cooldown::Column::Scope.eq(scope));
    }
    if let Some(user_id) = params.user_id {
        query = query.filter(entities::cooldown::Column::UserId.eq(user_id));
    }
    if let Some(key) = params.key.as_deref() {
        query = query.filter(entities::cooldown::Column::Key.eq(key));
    }

    let paginator = query.paginate(&*db, page_size);
    let pages = paginator
        .num_items_and_pages()
        .await
        .map_err(|e| ApiError::Internal(e.into()))?;
    let items = paginator
        .fetch_page(page)
        .await
        .map_err(|e| ApiError::Internal(e.into()))?;

    Ok(Json(PaginatedResponse {
        items: items.into_iter().map(CooldownDto::from).collect(),
        total: pages.number_of_items,
        page,
        page_size,
        total_pages: pages.number_of_pages,
    }))
}

#[utoipa::path(
    post,
    path = "/admin/cooldowns",
    request_body = UpsertCooldownRequest,
    responses(
        (status = 201, body = CooldownDto),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
    ),
    security(("user_jwt" = []), ("user_api_key" = []))
)]
#[axum::debug_handler]
pub async fn upsert_cooldown(
    _perm: RequirePermission<ManageCooldowns>,
    State(state): State<AppState>,
    Json(payload): Json<UpsertCooldownRequest>,
) -> CalibornResult<(StatusCode, Json<CooldownDto>)> {
    let db = state.service_registry.db_handle();

    // Replace any existing matching (scope, key, user_id) row.
    let mut delete_q = entities::cooldown::Entity::delete_many()
        .filter(entities::cooldown::Column::Scope.eq(&payload.scope))
        .filter(entities::cooldown::Column::Key.eq(&payload.key));
    delete_q = match payload.user_id {
        Some(uid) => delete_q.filter(entities::cooldown::Column::UserId.eq(uid)),
        None => delete_q.filter(entities::cooldown::Column::UserId.is_null()),
    };
    delete_q
        .exec(&*db)
        .await
        .map_err(|e| ApiError::Internal(e.into()))?;

    let inserted = entities::cooldown::ActiveModel {
        scope: Set(payload.scope),
        user_id: Set(payload.user_id),
        key: Set(payload.key),
        expires_at: Set(payload.expires_at.naive_utc()),
        ..Default::default()
    }
    .insert(&*db)
    .await
    .map_err(|e| ApiError::Internal(e.into()))?;

    Ok((StatusCode::CREATED, Json(inserted.into())))
}

#[utoipa::path(
    delete,
    path = "/admin/cooldowns/{id}",
    responses(
        (status = 204),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
    ),
    security(("user_jwt" = []), ("user_api_key" = []))
)]
#[axum::debug_handler]
pub async fn delete_cooldown(
    _perm: RequirePermission<ManageCooldowns>,
    State(state): State<AppState>,
    Path(id): Path<i32>,
) -> CalibornResult<StatusCode> {
    let db = state.service_registry.db_handle();
    entities::cooldown::Entity::delete_by_id(id)
        .exec(&*db)
        .await
        .map_err(|e| ApiError::Internal(e.into()))?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    delete,
    path = "/admin/cooldowns",
    params(CooldownBulkClearQuery),
    responses(
        (status = 200, body = CooldownBulkClearResponse),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
    ),
    security(("user_jwt" = []), ("user_api_key" = []))
)]
#[axum::debug_handler]
pub async fn bulk_clear_cooldowns(
    _perm: RequirePermission<ManageCooldowns>,
    State(state): State<AppState>,
    Query(params): Query<CooldownBulkClearQuery>,
) -> CalibornResult<Json<CooldownBulkClearResponse>> {
    let db = state.service_registry.db_handle();
    let mut query = entities::cooldown::Entity::delete_many();
    if let Some(scope) = params.scope.as_deref() {
        query = query.filter(entities::cooldown::Column::Scope.eq(scope));
    }
    if let Some(user_id) = params.user_id {
        query = query.filter(entities::cooldown::Column::UserId.eq(user_id));
    }
    if let Some(key) = params.key.as_deref() {
        query = query.filter(entities::cooldown::Column::Key.eq(key));
    }
    let res = query
        .exec(&*db)
        .await
        .map_err(|e| ApiError::Internal(e.into()))?;
    Ok(Json(CooldownBulkClearResponse {
        cleared: res.rows_affected,
    }))
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/cooldowns",
            get(list_cooldowns)
                .post(upsert_cooldown)
                .delete(bulk_clear_cooldowns),
        )
        .route("/cooldowns/{id}", delete(delete_cooldown))
}
