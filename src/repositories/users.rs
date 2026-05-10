use sea_orm::{ActiveValue, TransactionTrait, prelude::*};

use crate::{
    entities, generate_dtos,
    repositories::{ApplyQueryFilter, BaseRepository, RepositoryError},
};

/// A trait representing a repository for users.
#[async_trait::async_trait]
pub trait UserRepositoryExt: Send + Sync + 'static {
    /// Find a user with their associated channels.
    ///
    /// # Errors
    ///
    /// Returns a `RepositoryError` if something goes wrong while retrieving the
    /// user and their channels.
    async fn find_with_channels(
        &self,
        id: i64,
    ) -> Result<
        Option<(
            entities::users::Model,
            Vec<entities::connected_youtube_accounts::Model>,
        )>,
        RepositoryError,
    >;

    /// Find a user with their favourited songs.
    ///
    /// # Errors
    ///
    /// Returns a `RepositoryError` if something goes wrong while retrieving the
    /// user and their favourited songs.
    async fn find_with_favourites(
        &self,
        id: i64,
    ) -> Result<Option<(entities::users::Model, Vec<entities::songs::Model>)>, RepositoryError>;

    /// Check if a user has a specific song as a favourite.
    ///
    /// # Errors
    ///
    /// Returns a `RepositoryError` if something goes wrong while checking the
    /// user's favourites.
    async fn has_favourited_song(&self, id: i64, song_id: &str) -> Result<bool, RepositoryError>;

    /// Add a song as a favourite for a user.
    ///
    /// # Errors
    ///
    /// Returns a `RepositoryError` if something goes wrong while adding the
    /// favourite song.
    async fn add_favourite_song(&self, id: i64, song_id: &str) -> Result<(), RepositoryError>;

    /// Remove a song from a user's favourites.
    ///
    /// # Errors
    ///
    /// Returns a `RepositoryError` if something goes wrong while removing the
    /// favourite song.
    async fn remove_favourite_song(&self, id: i64, song_id: &str) -> Result<(), RepositoryError>;

    /// Add a linked channel to a user.
    ///
    /// # Errors
    ///
    /// Returns a `RepositoryError` if something goes wrong while adding the
    /// linked channel.
    async fn add_linked_channel(
        &self,
        id: i64,
        channel_id: String,
        channel_name: String,
    ) -> Result<(), RepositoryError>;

    /// Remove a linked channel from a user.
    ///
    /// # Errors
    ///
    /// Returns a `RepositoryError` if something goes wrong while removing the
    /// linked channel.
    async fn remove_linked_channel(
        &self,
        id: i64,
        channel_id: String,
    ) -> Result<(), RepositoryError>;

    /// Get the number of cans a user has.
    ///
    /// # Errors
    ///
    /// Returns a `RepositoryError` if something goes wrong while counting the
    /// user's cans.
    async fn can_count(&self, id: i64) -> Result<u64, RepositoryError>;

    /// Create an API key for a user.
    ///
    /// # Errors
    ///
    /// Returns a `RepositoryError` if something goes wrong while creating the
    /// API key.
    async fn create_api_key(
        &self,
        id: i64,
        key: &str,
        hash: &str,
        description: &str,
    ) -> Result<entities::api_keys::Model, RepositoryError>;

    /// Delete an API key for a user by their short API key.
    ///
    /// # Errors
    ///
    /// Returns a `RepositoryError` if something goes wrong while deleting the
    /// API key.
    async fn delete_api_key(&self, id: i64, key: &str) -> Result<(), RepositoryError>;

    /// Find a user by their short API key.
    ///
    /// # Errors
    ///
    /// Returns a `RepositoryError` if something goes wrong while retrieving the
    /// user by API key.
    async fn find_by_api_key(
        &self,
        key: &str,
    ) -> Result<Option<(entities::api_keys::Model, entities::users::Model)>, RepositoryError>;

    /// Transfers boonbucks from one user to another
    ///
    /// # Errors
    /// Returns an error if there is an issue with the database connection or
    /// if the user is not found
    async fn transfer_boonbucks(
        &self,
        _from_id: i64,
        _to_id: i64,
        _amount: i32,
    ) -> Result<(), RepositoryError> {
        unimplemented!()
    }
}

generate_dtos!(
    entities::users::Entity,
    CreateUserDto {
        id: i64
    },
    UpdateUserDto {
        id: Option<i64>,
        watched_time: Option<i64>,
        boonbucks: Option<i32>,
        migrated: Option<bool>,
        username: Option<Option<String>>,
        last_message_sent: Option<Option<DateTime>>,
    }
);

#[derive(Default)]
pub struct UserFilter {
    id: Option<i64>,
    username: Option<String>,
    migrated: Option<bool>,
    boonbucks: Option<i32>,
    watched_time: Option<i64>,
    last_message_sent: Option<Option<DateTime>>,
}

#[async_trait::async_trait]
impl ApplyQueryFilter<entities::users::Entity> for UserFilter {
    async fn apply(
        &self,
        query: Select<entities::users::Entity>,
    ) -> Select<entities::users::Entity> {
        let mut query = query;

        if let Some(id) = self.id {
            query = query.filter(entities::users::Column::Id.eq(id));
        }

        if let Some(username) = &self.username {
            query = query.filter(entities::users::Column::Username.eq(username));
        }

        if let Some(migrated) = self.migrated {
            query = query.filter(entities::users::Column::Migrated.eq(migrated));
        }

        if let Some(boonbucks) = self.boonbucks {
            query = query.filter(entities::users::Column::Boonbucks.eq(boonbucks));
        }

        if let Some(watched_time) = self.watched_time {
            query = query.filter(entities::users::Column::WatchedTime.eq(watched_time));
        }

        if let Some(last_message_sent) = self.last_message_sent {
            query = query.filter(entities::users::Column::LastMessageSent.eq(last_message_sent));
        }

        query
    }
}

pub struct UserToFavouriteSong;

impl Linked for UserToFavouriteSong {
    type FromEntity = entities::users::Entity;
    type ToEntity = entities::songs::Entity;

    fn link(&self) -> Vec<sea_orm::LinkDef> {
        vec![
            entities::favourite_songs::Relation::Users.def().rev(),
            entities::favourite_songs::Entity::belongs_to(entities::songs::Entity)
                .from(entities::favourite_songs::Column::SongId)
                .to(entities::songs::Column::FileHash)
                .into(),
        ]
    }
}

#[async_trait::async_trait]
impl UserRepositoryExt for BaseRepository<entities::users::Entity> {
    async fn find_with_channels(
        &self,
        id: i64,
    ) -> Result<
        Option<(
            entities::users::Model,
            Vec<entities::connected_youtube_accounts::Model>,
        )>,
        RepositoryError,
    > {
        let user = self.read(id).await?;

        if let Some(user) = user {
            let channels = user
                .find_related(entities::connected_youtube_accounts::Entity)
                .all(&self.db)
                .await?;
            Ok(Some((user, channels)))
        } else {
            Ok(None)
        }
    }

    async fn find_with_favourites(
        &self,
        id: i64,
    ) -> Result<Option<(entities::users::Model, Vec<entities::songs::Model>)>, RepositoryError>
    {
        let user = self.read(id).await?;

        if let Some(user) = user {
            let favourites = user.find_linked(UserToFavouriteSong).all(&self.db).await?;
            Ok(Some((user, favourites)))
        } else {
            Ok(None)
        }
    }

    async fn has_favourited_song(&self, id: i64, song_id: &str) -> Result<bool, RepositoryError> {
        entities::favourite_songs::Entity::find()
            .filter(entities::favourite_songs::Column::UserId.eq(id))
            .filter(entities::favourite_songs::Column::SongId.eq(song_id))
            .one(&self.db)
            .await
            .map(|song| song.is_some())
            .map_err(RepositoryError::from)
    }

    async fn add_favourite_song(&self, id: i64, song_id: &str) -> Result<(), RepositoryError> {
        if self.has_favourited_song(id, song_id).await? {
            return Ok(());
        }

        entities::favourite_songs::ActiveModel {
            user_id: ActiveValue::set(id),
            song_id: ActiveValue::set(song_id.to_string()),
            ..Default::default()
        }
        .insert(&self.db)
        .await?;

        Ok(())
    }

    async fn remove_favourite_song(&self, id: i64, song_id: &str) -> Result<(), RepositoryError> {
        if !self.has_favourited_song(id, song_id).await? {
            return Ok(());
        }

        entities::favourite_songs::Entity::delete_many()
            .filter(entities::favourite_songs::Column::UserId.eq(id))
            .filter(entities::favourite_songs::Column::SongId.eq(song_id))
            .exec(&self.db)
            .await?;

        Ok(())
    }

    async fn add_linked_channel(
        &self,
        id: i64,
        channel_id: String,
        channel_name: String,
    ) -> Result<(), RepositoryError> {
        entities::connected_youtube_accounts::ActiveModel {
            user_id: ActiveValue::set(id),
            youtube_channel_id: ActiveValue::set(channel_id),
            youtube_channel_name: ActiveValue::set(channel_name),
            ..Default::default()
        }
        .insert(&self.db)
        .await?;

        Ok(())
    }

    async fn remove_linked_channel(
        &self,
        id: i64,
        channel_id: String,
    ) -> Result<(), RepositoryError> {
        entities::connected_youtube_accounts::Entity::delete_many()
            .filter(entities::connected_youtube_accounts::Column::UserId.eq(id))
            .filter(entities::connected_youtube_accounts::Column::YoutubeChannelId.eq(channel_id))
            .exec(&self.db)
            .await?;

        Ok(())
    }

    async fn can_count(&self, id: i64) -> Result<u64, RepositoryError> {
        entities::cans::Entity::find()
            .filter(entities::cans::Column::AddedBy.eq(id))
            .filter(entities::cans::Column::Legit.eq(true))
            .count(&self.db)
            .await
            .map_err(RepositoryError::from)
    }

    async fn create_api_key(
        &self,
        id: i64,
        key: &str,
        hash: &str,
        description: &str,
    ) -> Result<entities::api_keys::Model, RepositoryError> {
        entities::api_keys::ActiveModel {
            user_id: ActiveValue::set(id),
            key: ActiveValue::set(key.to_string()),
            hash: ActiveValue::set(hash.to_string()),
            description: ActiveValue::set(description.to_string()),
            ..Default::default()
        }
        .insert(&self.db)
        .await
        .map_err(RepositoryError::from)
    }

    async fn delete_api_key(&self, id: i64, key: &str) -> Result<(), RepositoryError> {
        entities::api_keys::Entity::delete_many()
            .filter(entities::api_keys::Column::UserId.eq(id))
            .filter(entities::api_keys::Column::Key.eq(key))
            .exec(&self.db)
            .await?;

        Ok(())
    }

    async fn find_by_api_key(
        &self,
        key: &str,
    ) -> Result<Option<(entities::api_keys::Model, entities::users::Model)>, RepositoryError> {
        let api_key = entities::api_keys::Entity::find()
            .filter(entities::api_keys::Column::Key.eq(key))
            .one(&self.db)
            .await?;

        let Some(api_key) = api_key else {
            return Ok(None);
        };

        let user = api_key
            .find_related(entities::users::Entity)
            .one(&self.db)
            .await?;

        let Some(user) = user else {
            return Ok(None);
        };

        Ok(Some((api_key, user)))
    }

    /// Transfers boonbucks from one user to another in a single atomic transaction.
    ///
    /// # Note
    /// This method performs NO validation for:
    /// - Sufficient funds in the source account
    /// - Positive amount value
    /// - User existence
    ///
    /// The calling service is responsible for all business rule validations.
    /// This method only ensures the transfer happens atomically.
    async fn transfer_boonbucks(
        &self,
        from_id: i64,
        to_id: i64,
        amount: i32,
    ) -> Result<(), RepositoryError> {
        self.db
            .transaction(|txn| {
                Box::pin(async move {
                    let from_user = entities::users::Entity::find_by_id(from_id)
                        .one(txn)
                        .await?
                        .ok_or(DbErr::RecordNotFound("User not found".to_string()))?;

                    let to_user = entities::users::Entity::find_by_id(to_id)
                        .one(txn)
                        .await?
                        .ok_or(DbErr::RecordNotFound("User not found".to_string()))?;

                    entities::users::Entity::update(entities::users::ActiveModel {
                        id: ActiveValue::unchanged(from_id),
                        boonbucks: ActiveValue::set(from_user.boonbucks - amount),
                        ..Default::default()
                    })
                    .exec(txn)
                    .await?;

                    entities::users::Entity::update(entities::users::ActiveModel {
                        id: ActiveValue::unchanged(to_id),
                        boonbucks: ActiveValue::set(to_user.boonbucks + amount),
                        ..Default::default()
                    })
                    .exec(txn)
                    .await?;

                    Result::<_, DbErr>::Ok(())
                })
            })
            .await
            .map_err(RepositoryError::from)
    }
}
