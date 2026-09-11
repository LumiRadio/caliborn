//! Admin API DTOs.

use chrono::{DateTime, NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::entities;

#[derive(Serialize, ToSchema)]
pub struct AdminUserDto {
    pub id: i64,
    pub username: Option<String>,
    pub boonbucks: i32,
    pub watched_time: i64,
    pub migrated: bool,
    pub role: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub last_message_sent: Option<NaiveDateTime>,
}

impl From<entities::users::Model> for AdminUserDto {
    fn from(m: entities::users::Model) -> Self {
        Self {
            id: m.id,
            username: m.username,
            boonbucks: m.boonbucks,
            watched_time: m.watched_time,
            migrated: m.migrated,
            role: m.role,
            created_at: m.created_at,
            updated_at: m.updated_at,
            last_message_sent: m.last_message_sent,
        }
    }
}

#[derive(Deserialize, ToSchema, Default)]
pub struct UpdateUserRequest {
    pub boonbucks: Option<i32>,
    pub watched_time: Option<i64>,
    pub migrated: Option<bool>,
    pub username: Option<Option<String>>,
}

#[derive(Deserialize, IntoParams, Default)]
#[into_params(parameter_in = Query)]
pub struct UserListQuery {
    pub query: Option<String>,
    pub page: Option<u64>,
    pub page_size: Option<u64>,
}

#[derive(Serialize, ToSchema)]
pub struct PaginatedResponse<T> {
    pub items: Vec<T>,
    pub total: u64,
    pub page: u64,
    pub page_size: u64,
    pub total_pages: u64,
}

#[derive(Serialize, ToSchema)]
pub struct RoleDto {
    pub name: String,
    pub description: String,
    pub built_in: bool,
}

impl From<entities::roles::Model> for RoleDto {
    fn from(m: entities::roles::Model) -> Self {
        Self {
            name: m.name,
            description: m.description,
            built_in: m.built_in,
        }
    }
}

#[derive(Deserialize, ToSchema)]
pub struct CreateRoleRequest {
    pub name: String,
    pub description: String,
}

#[derive(Serialize, ToSchema)]
pub struct PermissionDto {
    pub name: String,
    pub description: String,
    pub built_in: bool,
}

impl From<entities::permissions::Model> for PermissionDto {
    fn from(m: entities::permissions::Model) -> Self {
        Self {
            name: m.name,
            description: m.description,
            built_in: m.built_in,
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct UserPermissionsDto {
    pub user_id: i64,
    pub role: String,
    pub direct_permissions: Vec<String>,
    pub effective_permissions: Vec<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct SetUserRoleRequest {
    /// `null` clears the role (resets to default `user`).
    pub role: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct CooldownDto {
    pub id: i32,
    pub scope: String,
    pub user_id: Option<i64>,
    pub key: String,
    pub expires_at: NaiveDateTime,
}

impl From<entities::cooldown::Model> for CooldownDto {
    fn from(m: entities::cooldown::Model) -> Self {
        Self {
            id: m.id,
            scope: m.scope,
            user_id: m.user_id,
            key: m.key,
            expires_at: m.expires_at,
        }
    }
}

#[derive(Deserialize, IntoParams, Default)]
#[into_params(parameter_in = Query)]
pub struct CooldownListQuery {
    pub scope: Option<String>,
    pub user_id: Option<i64>,
    pub key: Option<String>,
    pub page: Option<u64>,
    pub page_size: Option<u64>,
}

#[derive(Deserialize, ToSchema)]
pub struct UpsertCooldownRequest {
    pub scope: String,
    pub user_id: Option<i64>,
    pub key: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Deserialize, IntoParams, Default)]
#[into_params(parameter_in = Query)]
pub struct CooldownBulkClearQuery {
    pub scope: Option<String>,
    pub user_id: Option<i64>,
    pub key: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct CooldownBulkClearResponse {
    pub cleared: u64,
}

#[derive(Deserialize, ToSchema)]
pub struct SlcbLinkRequest {
    pub slcb_username: String,
}

#[derive(Deserialize, IntoParams, Default)]
#[into_params(parameter_in = Query)]
pub struct SlcbLinkQuery {
    pub force: Option<bool>,
}

#[derive(Serialize, ToSchema)]
pub struct SlcbLinkResponse {
    pub user_id: i64,
    pub slcb_username: String,
    pub hours_credited: i32,
    pub points_credited: i32,
    pub watched_time_after: i64,
    pub boonbucks_after: i32,
}

#[derive(Serialize, ToSchema)]
pub struct SlcbImportResponse {
    pub inserted: u64,
    pub updated: u64,
    pub skipped: u64,
}

#[derive(Serialize, ToSchema)]
pub struct SlcbMatchResponse {
    pub considered: u64,
    pub matched: u64,
    pub already_migrated: u64,
    pub no_slcb_row: u64,
}
