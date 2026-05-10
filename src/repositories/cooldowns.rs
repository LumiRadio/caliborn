use std::{fmt::Display, str::FromStr};

use sea_orm::{ColumnTrait, DeleteMany, QueryFilter, Select};

use crate::{
    entities, generate_dtos,
    repositories::{ApplyDeleteFilter, ApplyQueryFilter, BaseRepository},
};

use super::RepositoryError;

#[derive(Debug)]
pub struct UnknownCooldownScope(String);

impl Display for UnknownCooldownScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Unknown cooldown scope: {}", self.0)
    }
}

impl std::error::Error for UnknownCooldownScope {}

/// A cooldown scope
#[derive(Debug, Clone)]
pub enum CooldownScope {
    User,
    Global,
}

impl Display for CooldownScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CooldownScope::User => write!(f, "User"),
            CooldownScope::Global => write!(f, "Global"),
        }
    }
}

impl FromStr for CooldownScope {
    type Err = UnknownCooldownScope;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "User" => Ok(CooldownScope::User),
            "Global" => Ok(CooldownScope::Global),
            _ => Err(UnknownCooldownScope(s.to_string())),
        }
    }
}

generate_dtos!(
    entities::cooldown::Entity,
    CreateCooldownDto {
        scope: String,
        user_id: Option<i64>,
        key: String,
        expires_at: chrono::NaiveDateTime
    },
    UpdateCooldownDto {
        scope: Option<String>,
        user_id: Option<Option<i64>>,
        key: Option<String>,
        expires_at: Option<chrono::NaiveDateTime>
    }
);

#[derive(Default)]
pub struct CooldownFilter {
    scope: Option<CooldownScope>,
    key: Option<String>,
    user_id: Option<i64>,
    page: Option<u64>,
    page_size: Option<u64>,
}

#[derive(Default)]
pub struct CooldownDeleteFilter {
    scope: Option<CooldownScope>,
    key: Option<String>,
    user_id: Option<i64>,
}

impl CooldownFilter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn scope(mut self, scope: CooldownScope) -> Self {
        self.scope = Some(scope);
        self
    }

    pub fn key(mut self, key: String) -> Self {
        self.key = Some(key);
        self
    }

    pub fn user_id(mut self, user_id: i64) -> Self {
        self.user_id = Some(user_id);
        self
    }

    pub fn page(mut self, page: u64) -> Self {
        self.page = Some(page);
        self
    }

    pub fn page_size(mut self, page_size: u64) -> Self {
        self.page_size = Some(page_size);
        self
    }
}

#[async_trait::async_trait]
impl ApplyQueryFilter<entities::cooldown::Entity> for CooldownFilter {
    async fn apply(
        &self,
        query: Select<entities::cooldown::Entity>,
    ) -> Select<entities::cooldown::Entity> {
        let mut query = query;

        if let Some(scope) = &self.scope {
            query = query.filter(entities::cooldown::Column::Scope.eq(scope.to_string()));
        }

        if let Some(key) = &self.key {
            query = query.filter(entities::cooldown::Column::Key.eq(key));
        }

        if let Some(user_id) = self.user_id {
            query = query.filter(entities::cooldown::Column::UserId.eq(user_id));
        }

        query
    }

    fn page_size(&self) -> u64 {
        self.page_size.unwrap_or(20)
    }

    fn page(&self) -> u64 {
        self.page.unwrap_or(1)
    }
}

#[async_trait::async_trait]
impl ApplyDeleteFilter<entities::cooldown::Entity> for CooldownDeleteFilter {
    async fn apply_delete(
        &self,
        query: DeleteMany<entities::cooldown::Entity>,
    ) -> DeleteMany<entities::cooldown::Entity> {
        let mut query = query;

        if let Some(scope) = &self.scope {
            query = query.filter(entities::cooldown::Column::Scope.eq(scope.to_string()));
        }

        if let Some(key) = &self.key {
            query = query.filter(entities::cooldown::Column::Key.eq(key));
        }

        if let Some(user_id) = self.user_id {
            query = query.filter(entities::cooldown::Column::UserId.eq(user_id));
        }

        query
    }
}

/// A trait representing a repository for managing cooldowns.
#[async_trait::async_trait]
pub trait CooldownRepositoryExt: Send + Sync + 'static {
    /// Deletes a user cooldown by its key.
    async fn delete_user(&self, user_id: i64, key: &str) -> Result<(), RepositoryError>;

    /// Deletes a global cooldown by its key.
    async fn delete_global(&self, key: &str) -> Result<(), RepositoryError>;
}

#[async_trait::async_trait]
impl CooldownRepositoryExt for BaseRepository<entities::cooldown::Entity> {
    async fn delete_user(&self, user_id: i64, key: &str) -> Result<(), RepositoryError> {
        self.delete_by(CooldownDeleteFilter {
            key: Some(key.to_string()),
            scope: Some(CooldownScope::User),
            user_id: Some(user_id),
        })
        .await?;

        Ok(())
    }

    async fn delete_global(&self, key: &str) -> Result<(), RepositoryError> {
        self.delete_by(CooldownDeleteFilter {
            key: Some(key.to_string()),
            scope: Some(CooldownScope::Global),
            user_id: None,
        })
        .await?;

        Ok(())
    }
}
