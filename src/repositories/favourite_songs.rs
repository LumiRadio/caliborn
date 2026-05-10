use sea_orm::{ColumnTrait, DeleteMany, EntityTrait, PaginatorTrait, QueryFilter, Select};

use crate::{
    RepositoryError, entities, generate_dtos,
    repositories::{ApplyDeleteFilter, ApplyQueryFilter, BaseRepository},
};

#[async_trait::async_trait]
pub trait FavouriteSongRepositoryExt: Send + Sync + 'static {
    async fn is_favourited(&self, user_id: i64, song_id: &str) -> Result<bool, RepositoryError>;

    async fn remove_by_user_song(&self, user_id: i64, song_id: &str)
    -> Result<(), RepositoryError>;
}

generate_dtos!(
    entities::favourite_songs::Entity,
    CreateFavouriteSongDto {
        user_id: i64,
        song_id: String,
    },
    UpdateFavouriteSongDto {
        user_id: Option<i64>,
        song_id: Option<String>,
    }
);

#[derive(Default)]
pub struct FavouriteSongFilter {
    user_id: Option<i64>,
    song_id: Option<String>,
    page: Option<u64>,
    page_size: Option<u64>,
}

#[derive(Default)]
pub struct FavouriteSongDeleteFilter {
    user_id: Option<i64>,
    song_id: Option<String>,
}

#[async_trait::async_trait]
impl ApplyQueryFilter<entities::favourite_songs::Entity> for FavouriteSongFilter {
    async fn apply(
        &self,
        query: Select<entities::favourite_songs::Entity>,
    ) -> Select<entities::favourite_songs::Entity> {
        let mut query = query;

        if let Some(user_id) = self.user_id {
            query = query.filter(entities::favourite_songs::Column::UserId.eq(user_id));
        }

        if let Some(song_id) = &self.song_id {
            query = query.filter(entities::favourite_songs::Column::SongId.eq(song_id));
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
impl ApplyDeleteFilter<entities::favourite_songs::Entity> for FavouriteSongDeleteFilter {
    async fn apply_delete(
        &self,
        query: DeleteMany<entities::favourite_songs::Entity>,
    ) -> DeleteMany<entities::favourite_songs::Entity> {
        let mut query = query;

        if let Some(user_id) = self.user_id {
            query = query.filter(entities::favourite_songs::Column::UserId.eq(user_id));
        }

        if let Some(song_id) = &self.song_id {
            query = query.filter(entities::favourite_songs::Column::SongId.eq(song_id));
        }

        query
    }
}

#[async_trait::async_trait]
impl FavouriteSongRepositoryExt for BaseRepository<entities::favourite_songs::Entity> {
    async fn is_favourited(&self, user_id: i64, song_id: &str) -> Result<bool, RepositoryError> {
        entities::favourite_songs::Entity::find()
            .filter(entities::favourite_songs::Column::UserId.eq(user_id))
            .filter(entities::favourite_songs::Column::SongId.eq(song_id))
            .count(&self.db)
            .await
            .map_err(RepositoryError::from)
            .map(|count| count > 0)
    }

    async fn remove_by_user_song(
        &self,
        user_id: i64,
        song_id: &str,
    ) -> Result<(), RepositoryError> {
        self.delete_by(FavouriteSongDeleteFilter {
            user_id: Some(user_id),
            song_id: Some(song_id.to_string()),
        })
        .await?;

        Ok(()).map(|_| ())
    }
}
