use axum::{
    Router,
    extract::{Path, State},
    routing::{delete, get, put},
};
use reqwest::StatusCode;

use crate::{
    AppState,
    dtos::{
        Json,
        admin::{CreateRoleRequest, PermissionDto, RoleDto},
        error::{CalibornResult, ErrorResponse},
    },
    services::permissions::{ManagePermissions, RequirePermission},
};

#[utoipa::path(
    get,
    path = "/admin/roles",
    responses(
        (status = 200, body = Vec<RoleDto>),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
    ),
    security(("user_jwt" = []), ("user_api_key" = []))
)]
#[axum::debug_handler]
pub async fn list_roles(
    _perm: RequirePermission<ManagePermissions>,
    State(state): State<AppState>,
) -> CalibornResult<Json<Vec<RoleDto>>> {
    let roles = state
        .service_registry
        .permission_service()
        .list_roles()
        .await?;
    Ok(Json(roles.into_iter().map(RoleDto::from).collect()))
}

#[utoipa::path(
    post,
    path = "/admin/roles",
    request_body = CreateRoleRequest,
    responses(
        (status = 201, body = RoleDto),
        (status = 409, body = ErrorResponse),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
    ),
    security(("user_jwt" = []), ("user_api_key" = []))
)]
#[axum::debug_handler]
pub async fn create_role(
    _perm: RequirePermission<ManagePermissions>,
    State(state): State<AppState>,
    Json(payload): Json<CreateRoleRequest>,
) -> CalibornResult<(StatusCode, Json<RoleDto>)> {
    let role = state
        .service_registry
        .permission_service()
        .create_role(&payload.name, &payload.description)
        .await?;
    Ok((StatusCode::CREATED, Json(role.into())))
}

#[utoipa::path(
    delete,
    path = "/admin/roles/{name}",
    responses(
        (status = 204),
        (status = 404, body = ErrorResponse),
        (status = 409, body = ErrorResponse),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
    ),
    security(("user_jwt" = []), ("user_api_key" = []))
)]
#[axum::debug_handler]
pub async fn delete_role(
    _perm: RequirePermission<ManagePermissions>,
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> CalibornResult<StatusCode> {
    state
        .service_registry
        .permission_service()
        .delete_role(&name)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get,
    path = "/admin/roles/{name}/permissions",
    responses(
        (status = 200, body = Vec<String>),
        (status = 404, body = ErrorResponse),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
    ),
    security(("user_jwt" = []), ("user_api_key" = []))
)]
#[axum::debug_handler]
pub async fn list_role_permissions(
    _perm: RequirePermission<ManagePermissions>,
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> CalibornResult<Json<Vec<String>>> {
    let perms = state
        .service_registry
        .permission_service()
        .list_role_permissions(&name)
        .await?;
    Ok(Json(perms.into_iter().map(|p| p.permission).collect()))
}

#[utoipa::path(
    put,
    path = "/admin/roles/{name}/permissions/{perm}",
    responses(
        (status = 204),
        (status = 404, body = ErrorResponse),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
    ),
    security(("user_jwt" = []), ("user_api_key" = []))
)]
#[axum::debug_handler]
pub async fn attach_role_permission(
    _perm: RequirePermission<ManagePermissions>,
    State(state): State<AppState>,
    Path((name, perm_name)): Path<(String, String)>,
) -> CalibornResult<StatusCode> {
    state
        .service_registry
        .permission_service()
        .attach_permission_to_role(&name, &perm_name)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    delete,
    path = "/admin/roles/{name}/permissions/{perm}",
    responses(
        (status = 204),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
    ),
    security(("user_jwt" = []), ("user_api_key" = []))
)]
#[axum::debug_handler]
pub async fn detach_role_permission(
    _perm: RequirePermission<ManagePermissions>,
    State(state): State<AppState>,
    Path((name, perm_name)): Path<(String, String)>,
) -> CalibornResult<StatusCode> {
    state
        .service_registry
        .permission_service()
        .detach_permission_from_role(&name, &perm_name)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get,
    path = "/admin/permissions",
    responses(
        (status = 200, body = Vec<PermissionDto>),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
    ),
    security(("user_jwt" = []), ("user_api_key" = []))
)]
#[axum::debug_handler]
pub async fn list_permissions(
    _perm: RequirePermission<ManagePermissions>,
    State(state): State<AppState>,
) -> CalibornResult<Json<Vec<PermissionDto>>> {
    let perms = state
        .service_registry
        .permission_service()
        .list_permissions()
        .await?;
    Ok(Json(perms.into_iter().map(PermissionDto::from).collect()))
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/roles", get(list_roles).post(create_role))
        .route("/roles/{name}", delete(delete_role))
        .route("/roles/{name}/permissions", get(list_role_permissions))
        .route(
            "/roles/{name}/permissions/{perm}",
            put(attach_role_permission).delete(detach_role_permission),
        )
        .route("/permissions", get(list_permissions))
}
