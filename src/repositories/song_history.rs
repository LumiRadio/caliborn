use sea_orm::{QueryOrder, QuerySelect, prelude::*};

use crate::{
    entities, generate_dtos,
    repositories::{ApplyQueryFilter, BaseRepository, RepositoryError},
};

/// A trait representing a repository for song play history.
#[async_trait::async_trait]
pub trait SongHistoryRepositoryExt: Send + Sync + 'static {
    /// Get the play count for a specific song.
    ///
    /// # Errors
    ///
    /// Returns a `RepositoryError` if something goes wrong while counting the
    /// plays.
    async fn play_count(&self, file_hash: &str) -> Result<u64, RepositoryError>;

    async fn get_playing(&self) -> Result<Option<entities::played_songs::Model>, RepositoryError>;
}

generate_dtos!(
    entities::played_songs::Entity,
    CreatePlayedSongDto { song_id: String }
);

#[derive(Default)]
pub struct SongHistoryFilter {
    song_id: Option<String>,
    page: Option<u64>,
    page_size: Option<u64>,
}

#[async_trait::async_trait]
impl ApplyQueryFilter<entities::played_songs::Entity> for SongHistoryFilter {
    async fn apply(
        &self,
        query: Select<entities::played_songs::Entity>,
    ) -> Select<entities::played_songs::Entity> {
        let mut query = query;

        if let Some(song_id) = &self.song_id {
            query = query.filter(entities::played_songs::Column::SongId.eq(song_id));
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
impl SongHistoryRepositoryExt for BaseRepository<entities::played_songs::Entity> {
    async fn play_count(&self, file_hash: &str) -> Result<u64, RepositoryError> {
        entities::played_songs::Entity::find()
            .filter(entities::played_songs::Column::SongId.eq(file_hash))
            .count(&self.db)
            .await
            .map_err(RepositoryError::from)
    }

    async fn get_playing(&self) -> Result<Option<entities::played_songs::Model>, RepositoryError> {
        entities::played_songs::Entity::find()
            .order_by_desc(entities::played_songs::Column::PlayedAt)
            .limit(1)
            .one(&self.db)
            .await
            .map_err(RepositoryError::from)
    }
}
