use sea_orm::{ActiveValue, prelude::*};

use crate::{
    entities, generate_dtos,
    repositories::{ApplyQueryFilter, BaseRepository, RepositoryError},
};

/// A trait representing a repository for song tags.
#[async_trait::async_trait]
pub trait TagRepositoryExt: Send + Sync + 'static {
    /// Get all tags for a specific song.
    ///
    /// # Errors
    ///
    /// Returns a `RepositoryError` if something goes wrong while retrieving the
    /// song tags.
    async fn get_song_tags(
        &self,
        file_hash: &str,
    ) -> Result<Vec<entities::song_tags::Model>, RepositoryError>;

    /// Insert multiple tags for a song.
    ///
    /// # Errors
    ///
    /// Returns a `RepositoryError` if something goes wrong while inserting the
    /// tags.
    async fn insert_many(
        &self,
        file_hash: &str,
        tags: &[(&str, &str)],
    ) -> Result<(), RepositoryError>;

    /// Delete all tags for a specific song.
    ///
    /// # Errors
    ///
    /// Returns a `RepositoryError` if something goes wrong while deleting the
    /// tags.
    async fn delete_by_song(&self, file_hash: &str) -> Result<(), RepositoryError>;

    /// Prune tags for songs that no longer exist.
    ///
    /// # Errors
    ///
    /// Returns a `RepositoryError` if something goes wrong while pruning the
    /// tags.
    async fn prune(&self) -> Result<(), RepositoryError>;
}

generate_dtos!(
    entities::song_tags::Entity,
    CreateTagDto {
        song_id: String,
        tag: String,
        value: String,
    },
    UpdateTagDto {
        song_id: Option<String>,
        tag: Option<String>,
        value: Option<String>,
    }
);

#[derive(Default)]
pub struct TagFilter {
    song_id: Option<String>,
    tag: Option<String>,
    page: Option<u64>,
    page_size: Option<u64>,
}

#[async_trait::async_trait]
impl ApplyQueryFilter<entities::song_tags::Entity> for TagFilter {
    async fn apply(
        &self,
        query: Select<entities::song_tags::Entity>,
    ) -> Select<entities::song_tags::Entity> {
        let mut query = query;

        if let Some(song_id) = &self.song_id {
            query = query.filter(entities::song_tags::Column::SongId.eq(song_id));
        }

        if let Some(tag) = &self.tag {
            query = query.filter(entities::song_tags::Column::Tag.eq(tag));
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
impl TagRepositoryExt for BaseRepository<entities::song_tags::Entity> {
    async fn get_song_tags(
        &self,
        file_hash: &str,
    ) -> Result<Vec<entities::song_tags::Model>, RepositoryError> {
        entities::song_tags::Entity::find()
            .filter(entities::song_tags::Column::SongId.eq(file_hash))
            .all(&self.db)
            .await
            .map_err(RepositoryError::from)
    }

    async fn insert_many(
        &self,
        file_hash: &str,
        tags: &[(&str, &str)],
    ) -> Result<(), RepositoryError> {
        if tags.is_empty() {
            return Ok(());
        }

        let to_insert = tags
            .iter()
            .map(|(tag, value)| entities::song_tags::ActiveModel {
                song_id: ActiveValue::set(file_hash.to_string()),
                tag: ActiveValue::set(ToString::to_string(&tag)),
                value: ActiveValue::set(ToString::to_string(&value)),
                ..Default::default()
            })
            .collect::<Vec<_>>();

        entities::song_tags::Entity::insert_many(to_insert)
            .exec(&self.db)
            .await
            .map_err(RepositoryError::from)?;

        Ok(())
    }

    async fn delete_by_song(&self, file_hash: &str) -> Result<(), RepositoryError> {
        entities::song_tags::Entity::delete_many()
            .filter(entities::song_tags::Column::SongId.eq(file_hash))
            .exec(&self.db)
            .await
            .map_err(RepositoryError::from)?;

        Ok(())
    }

    async fn prune(&self) -> Result<(), RepositoryError> {
        entities::song_tags::Entity::delete_many()
            .exec(&self.db)
            .await
            .map_err(RepositoryError::from)?;

        Ok(())
    }
}
