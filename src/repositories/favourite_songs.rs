use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter};

use crate::{
    RepositoryError,
    dtos::page::{Page, PaginationParams},
    entities,
    repositories::AlwaysCloneableConnection,
};

#[async_trait::async_trait]
pub trait FavouriteSongRepository: Send + Sync + 'static {
    async fn insert(&self, user_id: i64, song_id: &str) -> Result<(), RepositoryError>;
    async fn delete(&self, user_id: i64, song_id: &str) -> Result<(), RepositoryError>;
    async fn get(
        &self,
        user_id: i64,
        pagination: &PaginationParams,
    ) -> Result<Page<entities::favourite_songs::Model>, RepositoryError>;
    async fn is_favourited(&self, user_id: i64, song_id: &str) -> Result<bool, RepositoryError>;
}

pub struct SeaOrmFavouriteSongRepository {
    db: AlwaysCloneableConnection,
}

impl SeaOrmFavouriteSongRepository {
    pub fn new(db: &AlwaysCloneableConnection) -> Self {
        Self { db: db.clone() }
    }
}

#[async_trait::async_trait]
impl FavouriteSongRepository for SeaOrmFavouriteSongRepository {
    async fn insert(&self, user_id: i64, song_id: &str) -> Result<(), RepositoryError> {
        entities::favourite_songs::ActiveModel {
            user_id: sea_orm::ActiveValue::Set(user_id),
            song_id: sea_orm::ActiveValue::Set(song_id.to_string()),
            ..Default::default()
        }
        .insert(&self.db)
        .await
        .map_err(RepositoryError::from)
        .map(|_model| ())
    }

    async fn delete(&self, user_id: i64, song_id: &str) -> Result<(), RepositoryError> {
        entities::favourite_songs::Entity::delete(entities::favourite_songs::ActiveModel {
            user_id: sea_orm::ActiveValue::Set(user_id),
            song_id: sea_orm::ActiveValue::Set(song_id.to_string()),
            ..Default::default()
        })
        .exec(&self.db)
        .await
        .map_err(RepositoryError::from)
        .map(|_model| ())
    }

    async fn get(
        &self,
        user_id: i64,
        pagination: &PaginationParams,
    ) -> Result<Page<entities::favourite_songs::Model>, RepositoryError> {
        let paginator = entities::favourite_songs::Entity::find()
            .filter(entities::favourite_songs::Column::UserId.eq(user_id))
            .paginate(&self.db, pagination.page_size);

        let page = paginator.fetch_page(pagination.page).await?;
        let total = paginator.num_items_and_pages().await?;

        Ok(Page::new(
            page,
            total.number_of_items,
            pagination.page,
            pagination.page_size,
            total.number_of_pages,
        ))
    }

    async fn is_favourited(&self, user_id: i64, song_id: &str) -> Result<bool, RepositoryError> {
        entities::favourite_songs::Entity::find()
            .filter(entities::favourite_songs::Column::UserId.eq(user_id))
            .filter(entities::favourite_songs::Column::SongId.eq(song_id))
            .count(&self.db)
            .await
            .map_err(RepositoryError::from)
            .map(|count| count > 0)
    }
}
