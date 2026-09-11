use std::{marker::PhantomData, str::FromStr};

use sea_orm::{EntityTrait, IntoActiveModel, PrimaryKeyTrait, Select};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;

use crate::{
    repositories::{ApplyQueryFilter, ApplyUpdates, BaseRepository, DatabaseConnection},
    services::admin::{
        error::AdminError,
        schema::{FieldMeta, schema_for},
    },
};

pub struct BrowseQuery {
    pub page: u64,
    pub page_size: u64,
}

#[async_trait::async_trait]
pub trait AdminResource: Send + Sync {
    fn name(&self) -> &str;
    fn schema(&self) -> Vec<FieldMeta>;

    async fn browse(&self, q: BrowseQuery) -> Result<Value, AdminError>;
    async fn read(&self, id: &str) -> Result<Option<Value>, AdminError>;
    async fn create(&self, body: Value) -> Result<Value, AdminError>;
    async fn edit(&self, id: &str, body: Value) -> Result<Value, AdminError>;
    async fn delete(&self, id: &str) -> Result<(), AdminError>;
}

pub struct EntityResource<E, C, U> {
    db: DatabaseConnection,
    name: String,
    _types: PhantomData<fn() -> (E, C, U)>,
}

impl<E, C, U> EntityResource<E, C, U>
where
    E: EntityTrait + Default + Send + Sync,
    E::Model: IntoActiveModel<E::ActiveModel> + Serialize + Send + Sync,
    E::ActiveModel: Send,
    E::Column: sea_orm::Iterable,
    <E::PrimaryKey as PrimaryKeyTrait>::ValueType: FromStr + ToString + Clone + Send,
    <<E::PrimaryKey as PrimaryKeyTrait>::ValueType as FromStr>::Err: std::fmt::Display,
    C: IntoActiveModel<E::ActiveModel> + DeserializeOwned + Send + Sync,
    U: ApplyUpdates<E::ActiveModel> + DeserializeOwned + Send + Sync,
{
    pub fn new(db: &DatabaseConnection, name: impl Into<String>) -> Self {
        Self {
            db: db.clone(),
            name: name.into(),
            _types: PhantomData,
        }
    }

    fn parse_id(
        &self,
        id: &str,
    ) -> Result<<E::PrimaryKey as PrimaryKeyTrait>::ValueType, AdminError> {
        id.parse().map_err(
            |e: <<E::PrimaryKey as PrimaryKeyTrait>::ValueType as FromStr>::Err| {
                AdminError::BadId(id.to_string(), e.to_string())
            },
        )
    }
}

pub struct GenericBrowseFilter<E> {
    pub page: u64,
    pub page_size: u64,
    pub _e: PhantomData<E>,
}

#[async_trait::async_trait]
impl<E: EntityTrait + Send + Sync> ApplyQueryFilter<E> for GenericBrowseFilter<E> {
    async fn apply(&self, query: Select<E>) -> Select<E> {
        query
    } // no filtering v1
    fn page_size(&self) -> u64 {
        self.page_size
    }
    fn page(&self) -> u64 {
        self.page
    }
}

#[async_trait::async_trait]
impl<E, C, U> AdminResource for EntityResource<E, C, U>
where
    E: EntityTrait + Default + Send + Sync,
    E::Model: IntoActiveModel<E::ActiveModel> + Serialize + Send + Sync,
    E::ActiveModel: Send,
    E::Column: sea_orm::Iterable,
    <E::PrimaryKey as PrimaryKeyTrait>::ValueType: FromStr + ToString + Clone + Send,
    <<E::PrimaryKey as PrimaryKeyTrait>::ValueType as FromStr>::Err: std::fmt::Display,
    C: IntoActiveModel<E::ActiveModel> + DeserializeOwned + Send + Sync,
    U: ApplyUpdates<E::ActiveModel> + DeserializeOwned + Send + Sync,
{
    fn name(&self) -> &str {
        &self.name
    }

    fn schema(&self) -> Vec<FieldMeta> {
        schema_for::<E>()
    }

    async fn browse(&self, q: BrowseQuery) -> Result<Value, AdminError> {
        let repo = BaseRepository::<E>::new(&self.db);
        let page = repo
            .browse(GenericBrowseFilter {
                page: q.page,
                page_size: q.page_size,
                _e: PhantomData,
            })
            .await?;
        Ok(serde_json::to_value(page).expect("Model: Serialize"))
    }

    async fn read(&self, id: &str) -> Result<Option<Value>, AdminError> {
        let repo = BaseRepository::<E>::new(&self.db);
        let model = repo.read(self.parse_id(id)?).await?;
        Ok(model.map(|m| serde_json::to_value(m).expect("Model: Serialize")))
    }

    async fn create(&self, body: Value) -> Result<Value, AdminError> {
        let dto: C =
            serde_json::from_value(body).map_err(|e| AdminError::Deserialize(e.to_string()))?;
        let repo = BaseRepository::<E>::new(&self.db);
        let model = repo.add(dto).await?;
        Ok(serde_json::to_value(model).expect("Model: Serialize"))
    }

    async fn edit(&self, id: &str, body: Value) -> Result<Value, AdminError> {
        let dto: U =
            serde_json::from_value(body).map_err(|e| AdminError::Deserialize(e.to_string()))?;
        let repo = BaseRepository::<E>::new(&self.db);
        let model = repo.edit(self.parse_id(id)?, dto).await?;
        Ok(serde_json::to_value(model).expect("Model: Serialize"))
    }

    async fn delete(&self, id: &str) -> Result<(), AdminError> {
        let repo = BaseRepository::<E>::new(&self.db);
        repo.delete(self.parse_id(id)?).await?;
        Ok(())
    }
}
