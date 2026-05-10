use sea_orm::prelude::*;

use crate::{
    entities, generate_dtos,
    repositories::{ApplyQueryFilter, BaseRepository, RepositoryError},
};

generate_dtos!(
    entities::server_channel_config::Entity,
    CreateServerChannelConfigDto {
        id: i64,
        server_id: i64,
    },
    UpdateServerChannelConfigDto {
        hydration_reminder: Option<bool>,
        allow_watch_time_accumulation: Option<bool>,
        allow_point_accumulation: Option<bool>,
        last_message_sent: Option<Option<chrono::NaiveDateTime>>,
    }
);

#[derive(Default)]
pub struct ServerChannelConfigFilter {
    server_id: Option<i64>,
    hydration_reminder: Option<bool>,
    allow_watch_time_accumulation: Option<bool>,
    allow_point_accumulation: Option<bool>,
    last_message_sent: Option<Option<chrono::NaiveDateTime>>,
    page: Option<u64>,
    page_size: Option<u64>,
}

#[async_trait::async_trait]
impl ApplyQueryFilter<entities::server_channel_config::Entity> for ServerChannelConfigFilter {
    async fn apply(
        &self,
        query: Select<entities::server_channel_config::Entity>,
    ) -> Select<entities::server_channel_config::Entity> {
        let mut query = query;

        if let Some(server_id) = self.server_id {
            query = query.filter(entities::server_channel_config::Column::ServerId.eq(server_id));
        }

        if let Some(hydration_reminder) = self.hydration_reminder {
            query = query.filter(
                entities::server_channel_config::Column::HydrationReminder.eq(hydration_reminder),
            );
        }

        if let Some(allow_watch_time_accumulation) = self.allow_watch_time_accumulation {
            query = query.filter(
                entities::server_channel_config::Column::AllowWatchTimeAccumulation
                    .eq(allow_watch_time_accumulation),
            );
        }

        if let Some(allow_point_accumulation) = self.allow_point_accumulation {
            query = query.filter(
                entities::server_channel_config::Column::AllowPointAccumulation
                    .eq(allow_point_accumulation),
            );
        }

        if let Some(last_message_sent) = self.last_message_sent {
            query = query.filter(
                entities::server_channel_config::Column::LastMessageSent.eq(last_message_sent),
            );
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

/// A trait representing a repository for server channel configurations.
#[async_trait::async_trait]
pub trait ServerChannelConfigRepositoryExt: Send + Sync + 'static {
    /// Find all channels with hydration reminders enabled.
    ///
    /// # Errors
    ///
    /// Returns a `RepositoryError` if something goes wrong while retrieving the
    /// hydration channels.
    async fn find_hydration_channels(
        &self,
    ) -> Result<Vec<entities::server_channel_config::Model>, RepositoryError>;
}

#[async_trait::async_trait]
impl ServerChannelConfigRepositoryExt for BaseRepository<entities::server_channel_config::Entity> {
    async fn find_hydration_channels(
        &self,
    ) -> Result<Vec<entities::server_channel_config::Model>, RepositoryError> {
        entities::server_channel_config::Entity::find()
            .filter(entities::server_channel_config::Column::HydrationReminder.eq(true))
            .all(&self.db)
            .await
            .map_err(RepositoryError::from)
    }
}
