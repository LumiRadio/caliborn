use axum::{
    Router,
    extract::{Path, State},
    routing::{get, put},
};
use reqwest::StatusCode;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, Set};

use crate::{
    AppState,
    dtos::{
        Json, Query,
        admin::{
            AdminUserDto, PaginatedResponse, SetUserRoleRequest, UpdateUserRequest, UserListQuery,
            UserPermissionsDto,
        },
        error::{ApiError, CalibornResult, ErrorResponse, PublicError},
    },
    entities,
    services::permissions::{ManagePermissions, ManageUsers, RequirePermission},
};

#[utoipa::path(
    get,
    path = "/admin/users",
    params(UserListQuery),
    responses(
        (status = 200, body = PaginatedResponse<AdminUserDto>),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
    ),
    security(("user_jwt" = []), ("user_api_key" = []))
)]
#[axum::debug_handler]
pub async fn list_users(
    _perm: RequirePermission<ManageUsers>,
    State(state): State<AppState>,
    Query(params): Query<UserListQuery>,
) -> CalibornResult<Json<PaginatedResponse<AdminUserDto>>> {
    let db = state.service_registry.db_handle();
    let page = params.page.unwrap_or(0).max(0);
    let page_size = params.page_size.unwrap_or(20).clamp(1, 200);

    let mut query = entities::users::Entity::find();
    if let Some(q) = params.query.as_deref().filter(|s| !s.is_empty()) {
        if let Ok(id) = q.parse::<i64>() {
            query = query.filter(entities::users::Column::Id.eq(id));
        } else {
            let pattern = format!("%{}%", q);
            query = query.filter(entities::users::Column::Username.like(pattern));
        }
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
        items: items.into_iter().map(AdminUserDto::from).collect(),
        total: pages.number_of_items,
        page,
        page_size,
        total_pages: pages.number_of_pages,
    }))
}

#[utoipa::path(
    get,
    path = "/admin/users/{id}",
    responses(
        (status = 200, body = AdminUserDto),
        (status = 404, body = ErrorResponse),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
    ),
    security(("user_jwt" = []), ("user_api_key" = []))
)]
#[axum::debug_handler]
pub async fn get_user(
    _perm: RequirePermission<ManageUsers>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> CalibornResult<Json<AdminUserDto>> {
    let db = state.service_registry.db_handle();
    let user = entities::users::Entity::find_by_id(id)
        .one(&*db)
        .await
        .map_err(|e| ApiError::Internal(e.into()))?
        .ok_or_else(|| {
            ApiError::Public(PublicError::with_owned(
                "user-not-found",
                format!("User {id} not found"),
                StatusCode::NOT_FOUND,
            ))
        })?;
    Ok(Json(user.into()))
}

#[utoipa::path(
    patch,
    path = "/admin/users/{id}",
    request_body = UpdateUserRequest,
    responses(
        (status = 200, body = AdminUserDto),
        (status = 404, body = ErrorResponse),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
    ),
    security(("user_jwt" = []), ("user_api_key" = []))
)]
#[axum::debug_handler]
pub async fn patch_user(
    _perm: RequirePermission<ManageUsers>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(payload): Json<UpdateUserRequest>,
) -> CalibornResult<Json<AdminUserDto>> {
    let db = state.service_registry.db_handle();
    let existing = entities::users::Entity::find_by_id(id)
        .one(&*db)
        .await
        .map_err(|e| ApiError::Internal(e.into()))?
        .ok_or_else(|| {
            ApiError::Public(PublicError::with_owned(
                "user-not-found",
                format!("User {id} not found"),
                StatusCode::NOT_FOUND,
            ))
        })?;

    let mut active: entities::users::ActiveModel = existing.into();
    if let Some(b) = payload.boonbucks {
        active.boonbucks = Set(b);
    }
    if let Some(w) = payload.watched_time {
        active.watched_time = Set(w);
    }
    if let Some(m) = payload.migrated {
        active.migrated = Set(m);
    }
    if let Some(u) = payload.username {
        active.username = Set(u);
    }

    let updated = active
        .update(&*db)
        .await
        .map_err(|e| ApiError::Internal(e.into()))?;
    Ok(Json(updated.into()))
}

#[utoipa::path(
    get,
    path = "/admin/users/{id}/permissions",
    responses(
        (status = 200, body = UserPermissionsDto),
        (status = 404, body = ErrorResponse),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
    ),
    security(("user_jwt" = []), ("user_api_key" = []))
)]
#[axum::debug_handler]
pub async fn get_user_permissions(
    _perm: RequirePermission<ManagePermissions>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> CalibornResult<Json<UserPermissionsDto>> {
    let db = state.service_registry.db_handle();
    let perm_service = state.service_registry.permission_service();

    let user = entities::users::Entity::find_by_id(id)
        .one(&*db)
        .await
        .map_err(|e| ApiError::Internal(e.into()))?
        .ok_or_else(|| {
            ApiError::Public(PublicError::with_owned(
                "user-not-found",
                format!("User {id} not found"),
                StatusCode::NOT_FOUND,
            ))
        })?;

    let direct = perm_service.list_user_permissions(id).await?;
    let effective = perm_service.effective_permissions(id).await?;

    let mut effective: Vec<String> = effective.into_iter().collect();
    effective.sort();

    Ok(Json(UserPermissionsDto {
        user_id: id,
        role: user.role,
        direct_permissions: direct
            .into_iter()
            .filter(|m| m.granted)
            .map(|m| m.permission)
            .collect(),
        effective_permissions: effective,
    }))
}

#[utoipa::path(
    put,
    path = "/admin/users/{id}/permissions/{perm}",
    responses(
        (status = 204),
        (status = 404, body = ErrorResponse),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
    ),
    security(("user_jwt" = []), ("user_api_key" = []))
)]
#[axum::debug_handler]
pub async fn grant_user_permission(
    _perm: RequirePermission<ManagePermissions>,
    State(state): State<AppState>,
    Path((id, perm_name)): Path<(i64, String)>,
) -> CalibornResult<StatusCode> {
    state
        .service_registry
        .permission_service()
        .grant_user_permission(id, &perm_name)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    delete,
    path = "/admin/users/{id}/permissions/{perm}",
    responses(
        (status = 204),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
    ),
    security(("user_jwt" = []), ("user_api_key" = []))
)]
#[axum::debug_handler]
pub async fn revoke_user_permission(
    _perm: RequirePermission<ManagePermissions>,
    State(state): State<AppState>,
    Path((id, perm_name)): Path<(i64, String)>,
) -> CalibornResult<StatusCode> {
    state
        .service_registry
        .permission_service()
        .revoke_user_permission(id, &perm_name)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    put,
    path = "/admin/users/{id}/role",
    request_body = SetUserRoleRequest,
    responses(
        (status = 204),
        (status = 404, body = ErrorResponse),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
    ),
    security(("user_jwt" = []), ("user_api_key" = []))
)]
#[axum::debug_handler]
pub async fn set_user_role(
    _perm: RequirePermission<ManagePermissions>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(payload): Json<SetUserRoleRequest>,
) -> CalibornResult<StatusCode> {
    state
        .service_registry
        .permission_service()
        .set_user_role(id, payload.role.as_deref())
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/users", get(list_users))
        .route("/users/{id}", get(get_user).patch(patch_user))
        .route("/users/{id}/permissions", get(get_user_permissions))
        .route(
            "/users/{id}/permissions/{perm}",
            put(grant_user_permission).delete(revoke_user_permission),
        )
        .route("/users/{id}/role", put(set_user_role))
}
