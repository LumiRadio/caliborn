use sea_orm::{DeleteMany, FromQueryResult, QueryFilter, QuerySelect, prelude::*};

use crate::{
    entities, generate_dtos,
    repositories::{ApplyDeleteFilter, ApplyQueryFilter, BaseRepository, RepositoryError},
};

/// A trait representing a repository for cans.
#[async_trait::async_trait]
pub trait CanRepositoryExt: Send + Sync + 'static {
    /// Count the number of cans in the database.
    ///
    /// # Errors
    ///
    /// Returns a `RepositoryError` if something goes wrong while counting the
    /// cans.
    async fn count(&self) -> Result<u64, RepositoryError>;
}

generate_dtos!(
    entities::cans::Entity,
    CreateCanDto {
        added_by: i64,
        legit: bool,
    },
    UpdateCanDto {
        legit: Option<bool>,
    }
);

#[derive(Default)]
pub struct CanFilter {
    added_by: Option<i64>,
    legit: Option<bool>,
    page: Option<u64>,
    page_size: Option<u64>,
}

#[derive(Default)]
pub struct CanDeleteFilter {
    added_by: Option<i64>,
    legit: Option<bool>,
}

#[async_trait::async_trait]
impl ApplyQueryFilter<entities::cans::Entity> for CanFilter {
    async fn apply(&self, query: Select<entities::cans::Entity>) -> Select<entities::cans::Entity> {
        let mut query = query;

        if let Some(added_by) = self.added_by {
            query = query.filter(entities::cans::Column::AddedBy.eq(added_by));
        }

        if let Some(legit) = self.legit {
            query = query.filter(entities::cans::Column::Legit.eq(legit));
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
impl ApplyDeleteFilter<entities::cans::Entity> for CanDeleteFilter {
    async fn apply_delete(
        &self,
        query: DeleteMany<entities::cans::Entity>,
    ) -> DeleteMany<entities::cans::Entity> {
        let mut query = query;

        if let Some(added_by) = self.added_by {
            query = query.filter(entities::cans::Column::AddedBy.eq(added_by));
        }

        if let Some(legit) = self.legit {
            query = query.filter(entities::cans::Column::Legit.eq(legit));
        }

        query
    }
}

#[async_trait::async_trait]
impl CanRepositoryExt for BaseRepository<entities::cans::Entity> {
    async fn count(&self) -> Result<u64, RepositoryError> {
        #[derive(FromQueryResult)]
        struct CountResult {
            count: u64,
        }

        entities::cans::Entity::find()
            .select_only()
            .column_as(entities::cans::Column::Id.count(), "count")
            .into_model::<CountResult>()
            .one(&self.db)
            .await
            .map(|count| count.map(|c| c.count).unwrap_or(0))
            .map_err(RepositoryError::from)
    }
}
