use sea_orm::{EntityTrait, IntoActiveModel, PrimaryKeyTrait};

use crate::{
    RepositoryError,
    dtos::page::Page,
    repositories::{ApplyQueryFilter, ApplyUpdates, BaseRepository, DatabaseConnection},
};

pub mod error;
pub mod registry;
pub mod resource;
pub mod schema;

#[derive(thiserror::Error, Debug)]
pub enum AdminCrudServiceError {
    #[error(transparent)]
    Repository(#[from] RepositoryError),
}

pub struct AdminCrudService {
    db: DatabaseConnection,
}

impl AdminCrudService {
    pub fn new(db: &DatabaseConnection) -> Self {
        Self { db: db.clone() }
    }

    pub async fn browse<E, F>(&self, filter: F) -> Result<Page<E::Model>, AdminCrudServiceError>
    where
        E: EntityTrait + Send + Sync + Default,
        E::Model: IntoActiveModel<E::ActiveModel> + Send + Sync,
        E::ActiveModel: Send,
        F: ApplyQueryFilter<E>,
    {
        let repo = BaseRepository::<E>::new(&self.db);
        repo.browse(filter).await.map_err(Into::into)
    }

    pub async fn read<E, T>(&self, id: T) -> Result<Option<E::Model>, AdminCrudServiceError>
    where
        E: EntityTrait + Send + Sync + Default,
        E::Model: IntoActiveModel<E::ActiveModel> + Send + Sync,
        E::ActiveModel: Send,
        T: Into<<E::PrimaryKey as PrimaryKeyTrait>::ValueType> + Send,
    {
        let repo = BaseRepository::<E>::new(&self.db);
        repo.read(id).await.map_err(Into::into)
    }

    pub async fn add<E, C>(&self, data: C) -> Result<E::Model, AdminCrudServiceError>
    where
        E: EntityTrait + Send + Sync + Default,
        E::Model: IntoActiveModel<E::ActiveModel> + Send + Sync,
        E::ActiveModel: Send,
        C: IntoActiveModel<E::ActiveModel> + Send + Sync,
    {
        let repo = BaseRepository::<E>::new(&self.db);
        repo.add(data).await.map_err(Into::into)
    }

    pub async fn edit<E, T, U>(&self, id: T, updates: U) -> Result<E::Model, AdminCrudServiceError>
    where
        E: EntityTrait + Send + Sync + Default,
        E::Model: IntoActiveModel<E::ActiveModel> + Send + Sync,
        E::ActiveModel: Send,
        T: Into<<E::PrimaryKey as PrimaryKeyTrait>::ValueType> + ToString + Send,
        U: ApplyUpdates<E::ActiveModel> + Send + Sync,
    {
        let repo = BaseRepository::<E>::new(&self.db);
        repo.edit(id, updates).await.map_err(Into::into)
    }

    pub async fn delete<E, T>(&self, id: T) -> Result<(), AdminCrudServiceError>
    where
        E: EntityTrait + Send + Sync + Default,
        E::Model: IntoActiveModel<E::ActiveModel> + Send + Sync,
        E::ActiveModel: Send,
        T: Into<<E::PrimaryKey as PrimaryKeyTrait>::ValueType> + Send,
    {
        let repo = BaseRepository::<E>::new(&self.db);
        repo.delete(id).await.map_err(Into::into)
    }
}
