use chrono::NaiveDateTime;
use sea_orm::{QueryOrder, QuerySelect, prelude::*};

use crate::{
    entities, generate_dtos,
    repositories::{ApplyQueryFilter, BaseRepository, RepositoryError},
};

/// A trait representing a repository for song requests.
#[async_trait::async_trait]
pub trait SongRequestRepositoryExt: Send + Sync + 'static {
    /// Get a list of recent song requests.
    ///
    /// # Errors
    ///
    /// Returns a `RepositoryError` if something goes wrong while retrieving the
    /// recent requests.
    async fn recent_requests(
        &self,
        limit: u64,
    ) -> Result<Vec<entities::song_requests::Model>, RepositoryError>;

    /// Get the timestamp of the last request for a specific song.
    /// Will return `None` if no requests have been made for the song.
    ///
    /// # Errors
    ///
    /// Returns a `RepositoryError` if something goes wrong while retrieving the
    /// timestamp.
    async fn last_request_timestamp(
        &self,
        file_hash: &str,
    ) -> Result<Option<NaiveDateTime>, RepositoryError>;

    /// Count the number of requests for a specific song.
    ///
    /// # Errors
    ///
    /// Returns a `RepositoryError` if something goes wrong while counting the
    /// requests.
    async fn count(&self, file_hash: &str) -> Result<u64, RepositoryError>;

    /// Count the total number of song requests.
    ///
    /// # Errors
    ///
    /// Returns a `RepositoryError` if something goes wrong while counting the
    /// total requests.
    async fn total_requests(&self) -> Result<u64, RepositoryError>;
}

generate_dtos!(
    entities::song_requests::Entity,
    CreateSongRequestDto {
        song_id: String,
        user_id: i64,
    }
);

#[derive(Default)]
pub struct SongRequestFilter {
    song_id: Option<String>,
    user_id: Option<i64>,
    page: Option<u64>,
    page_size: Option<u64>,
}

#[async_trait::async_trait]
impl ApplyQueryFilter<entities::song_requests::Entity> for SongRequestFilter {
    async fn apply(
        &self,
        query: Select<entities::song_requests::Entity>,
    ) -> Select<entities::song_requests::Entity> {
        let mut query = query;

        if let Some(song_id) = &self.song_id {
            query = query.filter(entities::song_requests::Column::SongId.eq(song_id));
        }

        if let Some(user_id) = self.user_id {
            query = query.filter(entities::song_requests::Column::UserId.eq(user_id));
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
impl SongRequestRepositoryExt for BaseRepository<entities::song_requests::Entity> {
    async fn recent_requests(
        &self,
        limit: u64,
    ) -> Result<Vec<entities::song_requests::Model>, RepositoryError> {
        entities::song_requests::Entity::find()
            .order_by_desc(entities::song_requests::Column::CreatedAt)
            .limit(limit)
            .all(&self.db)
            .await
            .map_err(RepositoryError::from)
    }

    async fn count(&self, file_hash: &str) -> Result<u64, RepositoryError> {
        entities::song_requests::Entity::find()
            .filter(entities::song_requests::Column::SongId.eq(file_hash))
            .count(&self.db)
            .await
            .map_err(RepositoryError::from)
    }

    async fn total_requests(&self) -> Result<u64, RepositoryError> {
        entities::song_requests::Entity::find()
            .count(&self.db)
            .await
            .map_err(RepositoryError::from)
    }

    async fn last_request_timestamp(
        &self,
        file_hash: &str,
    ) -> Result<Option<NaiveDateTime>, RepositoryError> {
        entities::song_requests::Entity::find()
            .filter(entities::song_requests::Column::SongId.eq(file_hash))
            .order_by_desc(entities::song_requests::Column::CreatedAt)
            .limit(1)
            .one(&self.db)
            .await
            .map_err(RepositoryError::from)
            .map(|model| model.map(|model| model.created_at))
    }
}
