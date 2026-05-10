use std::{marker::PhantomData, sync::Arc};

use sea_orm::{
    ActiveModelTrait, ConnectionTrait, DatabaseConnection, DeleteMany, EntityTrait,
    IntoActiveModel, PaginatorTrait, PrimaryKeyTrait, Select,
};

use crate::dtos::page::Page;

pub mod cans;
pub mod cooldowns;
pub mod favourite_songs;
pub mod permissions;
pub mod roles;
pub mod server_channel_config;
pub mod server_config;
pub mod song_history;
pub mod song_requests;
pub mod songs;
pub mod tags;
// Currently not in use, the frontend and bot are supposed to update user data
// pub mod token_storage;
pub mod users;

pub type DynError = Box<dyn std::error::Error + Send + Sync + 'static>;

pub struct AlwaysCloneableConnection(Arc<DatabaseConnection>);

impl Clone for AlwaysCloneableConnection {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl std::ops::Deref for AlwaysCloneableConnection {
    type Target = DatabaseConnection;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<DatabaseConnection> for AlwaysCloneableConnection {
    fn from(value: DatabaseConnection) -> Self {
        Self(Arc::new(value))
    }
}

#[async_trait::async_trait]
impl ConnectionTrait for AlwaysCloneableConnection {
    fn get_database_backend(&self) -> sea_orm::DbBackend {
        self.0.get_database_backend()
    }

    async fn execute(
        &self,
        stmt: sea_orm::Statement,
    ) -> Result<sea_orm::ExecResult, sea_orm::DbErr> {
        self.0.execute(stmt).await
    }

    async fn execute_unprepared(&self, sql: &str) -> Result<sea_orm::ExecResult, sea_orm::DbErr> {
        self.0.execute_unprepared(sql).await
    }

    async fn query_one(
        &self,
        stmt: sea_orm::Statement,
    ) -> Result<Option<sea_orm::QueryResult>, sea_orm::DbErr> {
        self.0.query_one(stmt).await
    }

    async fn query_all(
        &self,
        stmt: sea_orm::Statement,
    ) -> Result<Vec<sea_orm::QueryResult>, sea_orm::DbErr> {
        self.0.query_all(stmt).await
    }

    fn is_mock_connection(&self) -> bool {
        self.0.is_mock_connection()
    }

    fn support_returning(&self) -> bool {
        self.0.support_returning()
    }
}

/// An enum representing possible errors that can occur when interacting with a repository.
#[derive(thiserror::Error, Debug)]
pub enum RepositoryError {
    /// The entity that should be updated didn't exist
    #[error("The entity `{0}` with ID '{1}' does not exist to be updated")]
    UpdateNotFound(String, String),

    /// An error occurred while interacting with the repository. This IS a bug.
    #[error(transparent)]
    Unexpected(#[from] DynError),
}

impl From<sea_orm::DbErr> for RepositoryError {
    fn from(value: sea_orm::DbErr) -> Self {
        Self::Unexpected(anyhow::Error::new(value).context("sea-orm failure").into())
    }
}

impl<E: std::error::Error + Send + Sync + 'static> From<sea_orm::TransactionError<E>>
    for RepositoryError
{
    fn from(value: sea_orm::TransactionError<E>) -> Self {
        Self::Unexpected(
            anyhow::Error::new(value)
                .context("sea-orm transaction failure")
                .into(),
        )
    }
}

#[async_trait::async_trait]
pub trait ApplyQueryFilter<E>
where
    E: EntityTrait + Send + Sync,
{
    async fn apply(&self, query: Select<E>) -> Select<E>;

    fn page_size(&self) -> u64 {
        20
    }
    fn page(&self) -> u64 {
        1
    }
}

#[async_trait::async_trait]
pub trait ApplyDeleteFilter<E>
where
    E: EntityTrait + Send + Sync,
{
    async fn apply_delete(&self, query: DeleteMany<E>) -> DeleteMany<E>;
}

pub trait ApplyUpdates<T> {
    fn apply_to(self, target: &mut T);
}

pub struct BaseRepository<E> {
    db: AlwaysCloneableConnection,
    _phantom: PhantomData<E>,
}

impl<E> BaseRepository<E>
where
    E: EntityTrait + Send + Sync + Default,
    E::Model: IntoActiveModel<E::ActiveModel> + Send + Sync,
    E::ActiveModel: Send,
{
    pub fn new(db: &AlwaysCloneableConnection) -> Self {
        Self {
            db: db.clone(),
            _phantom: PhantomData,
        }
    }

    /// Browse entities with a given filter
    ///
    /// # Arguments
    ///
    /// * `filter` - The filter to apply to the query
    ///
    /// # Returns
    ///
    /// A `Result` containing a `Page` of entity models, or a `RepositoryError` if an error occurred
    pub async fn browse<F>(&self, filter: F) -> Result<Page<E::Model>, RepositoryError>
    where
        F: ApplyQueryFilter<E>,
    {
        let base_query = E::find();
        let filtered_query = filter.apply(base_query).await;

        let paginator = filtered_query.paginate(&self.db, filter.page_size());
        let items_and_pages = paginator.num_items_and_pages().await?;
        let items = paginator.fetch_page(filter.page()).await?;

        Ok(Page {
            items,
            page: filter.page(),
            page_size: filter.page_size(),
            total: items_and_pages.number_of_items,
            total_pages: items_and_pages.number_of_pages,
        })
    }

    /// Read an entity by its ID
    ///
    /// # Arguments
    ///
    /// * `id` - The ID of the entity to read
    ///
    /// # Returns
    ///
    /// A `Result` containing an `Option` of entity model, or a `RepositoryError` if an error occurred
    pub async fn read<T>(&self, id: T) -> Result<Option<E::Model>, RepositoryError>
    where
        T: Into<<E::PrimaryKey as PrimaryKeyTrait>::ValueType> + Send,
    {
        E::find_by_id(id).one(&self.db).await.map_err(Into::into)
    }

    /// Read an entity by a given filter
    ///
    /// # Arguments
    ///
    /// * `filter` - The filter to apply to the query
    ///
    /// # Returns
    ///
    /// A `Result` containing an `Option` of entity model, or a `RepositoryError` if an error occurred
    pub async fn read_by<F>(&self, filter: F) -> Result<Option<E::Model>, RepositoryError>
    where
        F: ApplyQueryFilter<E>,
    {
        let base_query = E::find();
        let filtered_query = filter.apply(base_query).await;

        filtered_query.one(&self.db).await.map_err(Into::into)
    }

    /// Edit an entity by its ID
    ///
    /// # Arguments
    ///
    /// * `id` - The ID of the entity to edit
    /// * `updates` - The updates to apply to the entity
    ///
    /// # Returns
    ///
    /// A `Result` containing the updated entity model, or a `RepositoryError` if an error occurred
    pub async fn edit<T, U>(&self, id: T, updates: U) -> Result<E::Model, RepositoryError>
    where
        T: Into<<E::PrimaryKey as PrimaryKeyTrait>::ValueType> + ToString + Send,
        U: ApplyUpdates<E::ActiveModel>,
    {
        let id_str = id.to_string();
        let existing: E::Model = self.read(id).await?.ok_or(RepositoryError::UpdateNotFound(
            E::default().as_str().to_string(),
            id_str,
        ))?;

        let mut active_model: E::ActiveModel = existing.into_active_model();
        updates.apply_to(&mut active_model);

        active_model.update(&self.db).await.map_err(Into::into)
    }

    /// Add a new entity
    ///
    /// # Arguments
    ///
    /// * `data` - The data to insert into the entity
    ///
    /// # Returns
    ///
    /// A `Result` containing the inserted entity model, or a `RepositoryError` if an error occurred
    pub async fn add<D>(&self, data: D) -> Result<E::Model, RepositoryError>
    where
        D: IntoActiveModel<E::ActiveModel> + Send,
    {
        let active_model: E::ActiveModel = data.into_active_model();
        active_model.insert(&self.db).await.map_err(Into::into)
    }

    pub async fn delete<T>(&self, id: T) -> Result<(), RepositoryError>
    where
        T: Into<<E::PrimaryKey as PrimaryKeyTrait>::ValueType> + Send,
    {
        E::delete_by_id(id).exec(&self.db).await?;

        Ok(())
    }

    pub async fn delete_by<F>(&self, filter: F) -> Result<(), RepositoryError>
    where
        F: ApplyDeleteFilter<E>,
    {
        let base_query = E::delete_many();
        let filtered_query = filter.apply_delete(base_query).await;

        filtered_query.exec(&self.db).await?;

        Ok(())
    }
}

/// Macro to generate common DTOs for an entity
///
/// # Usage
/// ```rust
/// generate_dtos!(
///     entities::cans::Entity,
///     CreateCanDto {
///         user_id: i64 => added_by,
///         legit: bool,
///     },
///     UpdateCanDto {
///         legit: Option<bool>,
///     }
/// );
/// ```
#[macro_export]
macro_rules! generate_dtos {
    (
        $entity:ty,
        $create_name:ident {
            $($create_field:ident: $create_type:ty $(=> $create_column:ident)?),* $(,)?
        }
    ) => {
        #[derive(Debug, Clone)]
        pub struct $create_name {
            $(pub $create_field: $create_type,)*
        }

        impl sea_orm::IntoActiveModel<<$entity as sea_orm::EntityTrait>::ActiveModel> for $create_name {
            fn into_active_model(self) -> <$entity as sea_orm::EntityTrait>::ActiveModel {
                let mut active_model = <<$entity as sea_orm::EntityTrait>::ActiveModel as Default>::default();

                $(
                    generate_dtos!(@set_create_field active_model, self.$create_field, $($create_column)?, $create_field);
                )*

                active_model
            }
        }
    };
    (
        $entity:ty,
        $create_name:ident {
            $($create_field:ident: $create_type:ty $(=> $create_column:ident)?),* $(,)?
        },
        $update_name:ident {
            $($update_field:ident: $update_type:ty $(=> $update_column:ident)?),* $(,)?
        }
    ) => {
        generate_dtos!($entity, $create_name {
            $($create_field: $create_type $(=> $create_column)?),*
        });

        #[derive(Debug, Clone, Default)]
        pub struct $update_name {
            $(pub $update_field: $update_type,)*
        }

        impl crate::repositories::ApplyUpdates<<$entity as sea_orm::EntityTrait>::ActiveModel> for $update_name {
            fn apply_to(self, active_model: &mut <$entity as sea_orm::EntityTrait>::ActiveModel) {
                $(
                    generate_dtos!(@set_update_field active_model, self.$update_field, $($update_column)?, $update_field);
                )*
            }
        }
    };

    (@set_create_field $active_model:ident, $value:expr, $column:ident, $field:ident) => {
        $active_model.$column = sea_orm::ActiveValue::set($value);
    };

    (@set_create_field $active_model:ident, $value:expr, , $field:ident) => {
        $active_model.$field = sea_orm::ActiveValue::set($value);
    };

    (@set_update_field $active_model:ident, $value:expr, $column:ident, $field:ident) => {
        if let Some(val) = $value {
            $active_model.$column = sea_orm::ActiveValue::set(val);
        }
    };

    (@set_update_field $active_model:ident, $value:expr, , $field:ident) => {
        if let Some(val) = $value {
            $active_model.$field = sea_orm::ActiveValue::set(val);
        }
    };
}
