//! Role / permission RBAC service and Axum extractor.
//!
//! The DB schema (`roles`, `permissions`, `role_permissions`, `user_permissions`,
//! `users.role`) is owned by the migration crate; this module reads/writes it.
//!
//! Effective permission set for a user = permissions attached to their role
//! ∪ direct `user_permissions` rows with `granted = true`. (The `granted`
//! column lets us model explicit revocations in the future; current behaviour
//! treats absence-or-false as "not granted".)

use std::{collections::HashSet, marker::PhantomData, sync::Arc};

use axum::extract::{FromRef, FromRequestParts};
use reqwest::StatusCode;
use sea_orm::{ActiveModelTrait, ActiveValue, ColumnTrait, EntityTrait, QueryFilter, Set};

use crate::{
    AppState, RepositoryError,
    dtos::error::{ApiError, PublicError, ToPublicError},
    entities,
    repositories::AlwaysCloneableConnection,
    services::auth::{Actor, AuthenticatedUser},
};

#[derive(Debug, thiserror::Error)]
pub enum PermissionServiceError {
    #[error("Role `{0}` not found")]
    RoleNotFound(String),
    #[error("Permission `{0}` not found")]
    PermissionNotFound(String),
    #[error("Built-in role `{0}` cannot be deleted")]
    BuiltInRoleProtected(String),
    #[error("Built-in permission `{0}` cannot be deleted")]
    BuiltInPermissionProtected(String),
    #[error("Role `{0}` already exists")]
    RoleAlreadyExists(String),
    #[error(transparent)]
    Db(#[from] sea_orm::DbErr),
    #[error(transparent)]
    Repository(#[from] RepositoryError),
}

impl ToPublicError for PermissionServiceError {
    fn as_public(&self) -> Option<PublicError> {
        match self {
            Self::RoleNotFound(_) | Self::PermissionNotFound(_) => Some(PublicError::with_owned(
                "not-found",
                self.to_string(),
                StatusCode::NOT_FOUND,
            )),
            Self::BuiltInRoleProtected(_) | Self::BuiltInPermissionProtected(_) => {
                Some(PublicError::with_owned(
                    "built-in-protected",
                    self.to_string(),
                    StatusCode::CONFLICT,
                ))
            }
            Self::RoleAlreadyExists(_) => Some(PublicError::with_owned(
                "role-already-exists",
                self.to_string(),
                StatusCode::CONFLICT,
            )),
            Self::Db(_) | Self::Repository(_) => None,
        }
    }
}

pub struct PermissionService {
    db: AlwaysCloneableConnection,
}

impl PermissionService {
    pub fn new(db: &AlwaysCloneableConnection) -> Self {
        Self { db: db.clone() }
    }

    /// Effective permissions for `user_id`: union of role-granted and
    /// direct-granted permission names.
    pub async fn effective_permissions(
        &self,
        user_id: i64,
    ) -> Result<HashSet<String>, PermissionServiceError> {
        let mut perms: HashSet<String> = HashSet::new();

        let user = entities::users::Entity::find_by_id(user_id)
            .one(&*self.db)
            .await?;

        if let Some(user) = user {
            let role_perms = entities::role_permissions::Entity::find()
                .filter(entities::role_permissions::Column::Role.eq(user.role))
                .all(&*self.db)
                .await?;
            for rp in role_perms {
                perms.insert(rp.permission);
            }
        }

        let user_perms = entities::user_permissions::Entity::find()
            .filter(entities::user_permissions::Column::UserId.eq(user_id))
            .filter(entities::user_permissions::Column::Granted.eq(true))
            .all(&*self.db)
            .await?;
        for up in user_perms {
            perms.insert(up.permission);
        }

        Ok(perms)
    }

    pub async fn list_roles(&self) -> Result<Vec<entities::roles::Model>, PermissionServiceError> {
        Ok(entities::roles::Entity::find().all(&*self.db).await?)
    }

    pub async fn list_permissions(
        &self,
    ) -> Result<Vec<entities::permissions::Model>, PermissionServiceError> {
        Ok(entities::permissions::Entity::find().all(&*self.db).await?)
    }

    pub async fn list_role_permissions(
        &self,
        role: &str,
    ) -> Result<Vec<entities::role_permissions::Model>, PermissionServiceError> {
        self.require_role(role).await?;
        Ok(entities::role_permissions::Entity::find()
            .filter(entities::role_permissions::Column::Role.eq(role))
            .all(&*self.db)
            .await?)
    }

    pub async fn list_user_permissions(
        &self,
        user_id: i64,
    ) -> Result<Vec<entities::user_permissions::Model>, PermissionServiceError> {
        Ok(entities::user_permissions::Entity::find()
            .filter(entities::user_permissions::Column::UserId.eq(user_id))
            .all(&*self.db)
            .await?)
    }

    pub async fn create_role(
        &self,
        name: &str,
        description: &str,
    ) -> Result<entities::roles::Model, PermissionServiceError> {
        if entities::roles::Entity::find_by_id(name.to_string())
            .one(&*self.db)
            .await?
            .is_some()
        {
            return Err(PermissionServiceError::RoleAlreadyExists(name.to_string()));
        }

        let model = entities::roles::ActiveModel {
            name: Set(name.to_string()),
            description: Set(description.to_string()),
            built_in: Set(false),
        };
        let inserted = entities::roles::Entity::insert(model)
            .exec_with_returning(&*self.db)
            .await?;
        Ok(inserted)
    }

    pub async fn delete_role(&self, name: &str) -> Result<(), PermissionServiceError> {
        let role = self.require_role(name).await?;
        if role.built_in {
            return Err(PermissionServiceError::BuiltInRoleProtected(
                name.to_string(),
            ));
        }
        entities::role_permissions::Entity::delete_many()
            .filter(entities::role_permissions::Column::Role.eq(name))
            .exec(&*self.db)
            .await?;
        entities::roles::Entity::delete_by_id(name.to_string())
            .exec(&*self.db)
            .await?;
        Ok(())
    }

    pub async fn attach_permission_to_role(
        &self,
        role: &str,
        permission: &str,
    ) -> Result<(), PermissionServiceError> {
        self.require_role(role).await?;
        self.require_permission(permission).await?;

        let exists = entities::role_permissions::Entity::find()
            .filter(entities::role_permissions::Column::Role.eq(role))
            .filter(entities::role_permissions::Column::Permission.eq(permission))
            .one(&*self.db)
            .await?;
        if exists.is_some() {
            return Ok(());
        }

        entities::role_permissions::ActiveModel {
            role: Set(role.to_string()),
            permission: Set(permission.to_string()),
        }
        .insert(&*self.db)
        .await?;
        Ok(())
    }

    pub async fn detach_permission_from_role(
        &self,
        role: &str,
        permission: &str,
    ) -> Result<(), PermissionServiceError> {
        entities::role_permissions::Entity::delete_many()
            .filter(entities::role_permissions::Column::Role.eq(role))
            .filter(entities::role_permissions::Column::Permission.eq(permission))
            .exec(&*self.db)
            .await?;
        Ok(())
    }

    pub async fn grant_user_permission(
        &self,
        user_id: i64,
        permission: &str,
    ) -> Result<(), PermissionServiceError> {
        self.require_permission(permission).await?;

        let existing = entities::user_permissions::Entity::find()
            .filter(entities::user_permissions::Column::UserId.eq(user_id))
            .filter(entities::user_permissions::Column::Permission.eq(permission))
            .one(&*self.db)
            .await?;

        if let Some(row) = existing {
            if row.granted {
                return Ok(());
            }
            entities::user_permissions::ActiveModel {
                user_id: ActiveValue::unchanged(row.user_id),
                permission: ActiveValue::unchanged(row.permission),
                granted: Set(true),
            }
            .update(&*self.db)
            .await?;
        } else {
            entities::user_permissions::ActiveModel {
                user_id: Set(user_id),
                permission: Set(permission.to_string()),
                granted: Set(true),
            }
            .insert(&*self.db)
            .await?;
        }
        Ok(())
    }

    pub async fn revoke_user_permission(
        &self,
        user_id: i64,
        permission: &str,
    ) -> Result<(), PermissionServiceError> {
        entities::user_permissions::Entity::delete_many()
            .filter(entities::user_permissions::Column::UserId.eq(user_id))
            .filter(entities::user_permissions::Column::Permission.eq(permission))
            .exec(&*self.db)
            .await?;
        Ok(())
    }

    pub async fn set_user_role(
        &self,
        user_id: i64,
        role: Option<&str>,
    ) -> Result<(), PermissionServiceError> {
        let role_value = match role {
            Some(name) => {
                self.require_role(name).await?;
                name.to_string()
            }
            None => "user".to_string(),
        };

        entities::users::Entity::update_many()
            .col_expr(
                entities::users::Column::Role,
                sea_orm::sea_query::Expr::value(role_value),
            )
            .filter(entities::users::Column::Id.eq(user_id))
            .exec(&*self.db)
            .await?;
        Ok(())
    }

    async fn require_role(
        &self,
        name: &str,
    ) -> Result<entities::roles::Model, PermissionServiceError> {
        entities::roles::Entity::find_by_id(name.to_string())
            .one(&*self.db)
            .await?
            .ok_or_else(|| PermissionServiceError::RoleNotFound(name.to_string()))
    }

    async fn require_permission(
        &self,
        name: &str,
    ) -> Result<entities::permissions::Model, PermissionServiceError> {
        entities::permissions::Entity::find_by_id(name.to_string())
            .one(&*self.db)
            .await?
            .ok_or_else(|| PermissionServiceError::PermissionNotFound(name.to_string()))
    }
}

/// Trait implemented by marker types that name a single permission.
///
/// Used by [`RequirePermission`] to gate routes at compile time.
pub trait PermissionMarker: Send + Sync + 'static {
    const NAME: &'static str;
}

/// Axum extractor that fails the request with 403 unless the authenticated
/// actor has permission `P`.
///
/// Depends on the [`authenticate`](crate::services::auth::authenticate)
/// middleware running first (to populate the [`Actor`] extension).
///
/// Effective permissions for the actor are loaded once per request and stored
/// in the request extensions, so multiple `RequirePermission` extractors on
/// the same route only hit the DB once.
pub struct RequirePermission<P: PermissionMarker>(pub PhantomData<P>);

#[derive(Clone)]
struct CachedPermissions(Arc<HashSet<String>>);

impl<S, P> FromRequestParts<S> for RequirePermission<P>
where
    S: Send + Sync,
    AppState: FromRef<S>,
    P: PermissionMarker,
{
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        state: &S,
    ) -> Result<Self, Self::Rejection> {
        let actor = parts
            .extensions
            .get::<Actor>()
            .cloned()
            .ok_or(ApiError::Internal(anyhow::anyhow!(
                "missing actor extension (perhaps `authenticate` middleware is not used)"
            )))?;

        let _ = AuthenticatedUser(actor.clone());

        let cached = parts.extensions.get::<CachedPermissions>().cloned();
        let perms = match cached {
            Some(c) => c.0,
            None => {
                let app_state = AppState::from_ref(state);
                let service = app_state.service_registry.permission_service();
                let user_id: i64 = actor.user_id().into();
                let perms = service.effective_permissions(user_id).await?;
                let arc = Arc::new(perms);
                parts.extensions.insert(CachedPermissions(Arc::clone(&arc)));
                arc
            }
        };

        if !perms.contains(P::NAME) {
            return Err(ApiError::Public(PublicError::with_owned(
                "forbidden",
                format!("Missing permission: {}", P::NAME),
                StatusCode::FORBIDDEN,
            )));
        }

        Ok(Self(PhantomData))
    }
}

/// Define a marker type for a permission name.
///
/// Used in admin route handlers to declare which permission gates the route:
///
/// ```ignore
/// define_permission!(ManageUsers => shared_constants::permissions::PERM_MANAGE_USERS.name);
/// async fn handler(_perm: RequirePermission<ManageUsers>) { ... }
/// ```
#[macro_export]
macro_rules! define_permission {
    ($vis:vis $marker:ident => $name:expr) => {
        $vis struct $marker;
        impl $crate::services::permissions::PermissionMarker for $marker {
            const NAME: &'static str = $name;
        }
    };
}

// Marker types for the built-in admin permissions. Re-exported below for
// ergonomic use in admin routes.
define_permission!(pub ManageUsers => shared_constants::permissions::PERM_MANAGE_USERS.name);
define_permission!(pub ManagePermissions => shared_constants::permissions::PERM_MANAGE_PERMISSIONS.name);
define_permission!(pub ManageCooldowns => shared_constants::permissions::PERM_MANAGE_COOLDOWNS.name);
define_permission!(pub ManageSlcb => shared_constants::permissions::PERM_MANAGE_SLCB.name);
