use axum::{
    extract::{Path, Query, State},
    routing::get,
};
use serde_json::Value;

use crate::{
    AppState,
    dtos::{
        Json,
        error::{CalibornResult, ErrorResponse},
        page::PaginationParams,
    },
    services::{
        admin::{resource::BrowseQuery, schema::FieldMeta},
        permissions::{RequirePermission, UseCrud},
    },
};

/// List a page of records for `resource`.
#[utoipa::path(
    get,
    path = "/admin/crud/{resource}",
    params(("resource" = String, Path, description = "Resource name, as listed by `GET /admin/crud/_meta`"), PaginationParams),
    responses(
        (status = 200, description = "A page of records; shape depends on the resource", body = serde_json::Value),
        (status = 401, description = "Missing or invalid credentials", body = ErrorResponse),
        (status = 403, description = "Caller lacks the `use_admin_crud` permission", body = ErrorResponse),
        (status = 404, description = "Unknown resource", body = ErrorResponse),
        (status = 500, description = "An internal server error occurred", body = ErrorResponse),
    ),
    tags = ["Admin", "CRUD"],
    security(("user_jwt" = []), ("user_api_key" = []))
)]
pub async fn list(
    _p: RequirePermission<UseCrud>,
    State(s): State<AppState>,
    Path(resource): Path<String>,
    Query(pg): Query<PaginationParams>,
) -> CalibornResult<Json<Value>> {
    let reg = s.service_registry.admin_registry();
    let res = reg.get(&resource)?;
    Ok(Json(
        res.browse(BrowseQuery {
            page: pg.page,
            page_size: pg.page_size,
        })
        .await?,
    ))
}

/// Read a single record by primary key.
#[utoipa::path(
    get,
    path = "/admin/crud/{resource}/{id}",
    params(("resource" = String, Path, description = "Resource name, as listed by `GET /admin/crud/_meta`"), ("id" = String, Path, description = "Primary key of the record, as a string")),
    responses(
        (status = 200, description = "The record, or `null` if no row has that id", body = serde_json::Value),
        (status = 401, description = "Missing or invalid credentials", body = ErrorResponse),
        (status = 403, description = "Caller lacks the `use_admin_crud` permission", body = ErrorResponse),
        (status = 404, description = "Unknown resource", body = ErrorResponse),
        (status = 400, description = "Id could not be parsed into the resource's key type", body = ErrorResponse),
        (status = 500, description = "An internal server error occurred", body = ErrorResponse),
    ),
    tags = ["Admin", "CRUD"],
    security(("user_jwt" = []), ("user_api_key" = []))
)]
pub async fn read(
    _p: RequirePermission<UseCrud>,
    State(s): State<AppState>,
    Path((resource, id)): Path<(String, String)>,
) -> CalibornResult<Json<Option<Value>>> {
    let reg = s.service_registry.admin_registry();
    let res = reg.get(&resource)?;
    Ok(Json(res.read(&id).await?))
}

/// Create a record. The body is validated against the resource's create DTO.
#[utoipa::path(
    post,
    path = "/admin/crud/{resource}",
    params(("resource" = String, Path, description = "Resource name, as listed by `GET /admin/crud/_meta`")),
    request_body(content = serde_json::Value, description = "Resource-specific create payload; see `GET /admin/crud/{resource}/_schema`"),
    responses(
        (status = 200, description = "The created record", body = serde_json::Value),
        (status = 401, description = "Missing or invalid credentials", body = ErrorResponse),
        (status = 403, description = "Caller lacks the `use_admin_crud` permission", body = ErrorResponse),
        (status = 404, description = "Unknown resource", body = ErrorResponse),
        (status = 400, description = "Payload did not match the resource's create DTO", body = ErrorResponse),
        (status = 500, description = "An internal server error occurred", body = ErrorResponse),
    ),
    tags = ["Admin", "CRUD"],
    security(("user_jwt" = []), ("user_api_key" = []))
)]
pub async fn create(
    _p: RequirePermission<UseCrud>,
    State(s): State<AppState>,
    Path(resource): Path<String>,
    Json(data): Json<Value>,
) -> CalibornResult<Json<Value>> {
    let reg = s.service_registry.admin_registry();
    let res = reg.get(&resource)?;
    Ok(Json(res.create(data).await?))
}

/// Partially update a record. Omitted fields keep their current value.
#[utoipa::path(
    put,
    path = "/admin/crud/{resource}/{id}",
    params(("resource" = String, Path, description = "Resource name, as listed by `GET /admin/crud/_meta`"), ("id" = String, Path, description = "Primary key of the record, as a string")),
    request_body(content = serde_json::Value, description = "Resource-specific update payload; every field is optional"),
    responses(
        (status = 200, description = "The updated record", body = serde_json::Value),
        (status = 401, description = "Missing or invalid credentials", body = ErrorResponse),
        (status = 403, description = "Caller lacks the `use_admin_crud` permission", body = ErrorResponse),
        (status = 404, description = "Unknown resource", body = ErrorResponse),
        (status = 400, description = "Payload did not match the resource's update DTO, or the id was unparseable", body = ErrorResponse),
        (status = 500, description = "An internal server error occurred", body = ErrorResponse),
    ),
    tags = ["Admin", "CRUD"],
    security(("user_jwt" = []), ("user_api_key" = []))
)]
pub async fn edit(
    _p: RequirePermission<UseCrud>,
    State(s): State<AppState>,
    Path((resource, id)): Path<(String, String)>,
    Json(data): Json<Value>,
) -> CalibornResult<Json<Value>> {
    let reg = s.service_registry.admin_registry();
    let res = reg.get(&resource)?;
    Ok(Json(res.edit(&id, data).await?))
}

/// Delete a record by primary key.
#[utoipa::path(
    delete,
    path = "/admin/crud/{resource}/{id}",
    params(("resource" = String, Path, description = "Resource name, as listed by `GET /admin/crud/_meta`"), ("id" = String, Path, description = "Primary key of the record, as a string")),
    responses(
        (status = 200, description = "Record deleted"),
        (status = 401, description = "Missing or invalid credentials", body = ErrorResponse),
        (status = 403, description = "Caller lacks the `use_admin_crud` permission", body = ErrorResponse),
        (status = 404, description = "Unknown resource", body = ErrorResponse),
        (status = 400, description = "Id could not be parsed into the resource's key type", body = ErrorResponse),
        (status = 500, description = "An internal server error occurred", body = ErrorResponse),
    ),
    tags = ["Admin", "CRUD"],
    security(("user_jwt" = []), ("user_api_key" = []))
)]
pub async fn delete(
    _p: RequirePermission<UseCrud>,
    State(s): State<AppState>,
    Path((resource, id)): Path<(String, String)>,
) -> CalibornResult<Json<()>> {
    let reg = s.service_registry.admin_registry();
    let res = reg.get(&resource)?;
    res.delete(&id).await?;
    Ok(Json(()))
}

/// Column metadata for `resource`
#[utoipa::path(
    get,
    path = "/admin/crud/{resource}/_schema",
    params(("resource" = String, Path, description = "Resource name, as listed by `GET /admin/crud/_meta`")),
    responses(
        (status = 200, description = "One entry per column", body = Vec<FieldMeta>),
        (status = 401, description = "Missing or invalid credentials", body = ErrorResponse),
        (status = 403, description = "Caller lacks the `use_admin_crud` permission", body = ErrorResponse),
        (status = 404, description = "Unknown resource", body = ErrorResponse),
        (status = 500, description = "An internal server error occurred", body = ErrorResponse),
    ),
    tags = ["Admin", "CRUD"],
    security(("user_jwt" = []), ("user_api_key" = []))
)]
pub async fn schema(
    _p: RequirePermission<UseCrud>,
    State(s): State<AppState>,
    Path(resource): Path<String>,
) -> CalibornResult<Json<Vec<FieldMeta>>> {
    let reg = s.service_registry.admin_registry();
    let res = reg.get(&resource)?;
    Ok(Json(res.schema()))
}

/// Every resource name the registry serves.
#[utoipa::path(
    get,
    path = "/admin/crud/_meta",
    responses(
        (status = 200, description = "Registered resource names", body = Vec<String>),
        (status = 401, description = "Missing or invalid credentials", body = ErrorResponse),
        (status = 403, description = "Caller lacks the `use_admin_crud` permission", body = ErrorResponse),
        (status = 500, description = "An internal server error occurred", body = ErrorResponse),
    ),
    tags = ["Admin", "CRUD"],
    security(("user_jwt" = []), ("user_api_key" = []))
)]
pub async fn meta(
    _p: RequirePermission<UseCrud>,
    State(s): State<AppState>,
) -> CalibornResult<Json<Vec<String>>> {
    let reg = s.service_registry.admin_registry();
    Ok(Json(
        reg.names().into_iter().map(|s| s.to_string()).collect(),
    ))
}

pub fn routes() -> axum::Router<AppState> {
    axum::Router::new()
        .route("/crud/{resource}", get(list).post(create))
        .route("/crud/{resource}/_schema", get(schema))
        .route("/crud/{resource}/{id}", get(read).put(edit).delete(delete))
        .route("/crud/_meta", get(meta))
}
