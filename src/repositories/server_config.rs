use sea_orm::prelude::*;

use crate::{entities, generate_dtos, repositories::ApplyQueryFilter};

generate_dtos!(
    entities::server_config::Entity,
    CreateServerConfigDto {
        id: i64
    },
    UpdateServerConfigDto {
        slot_jackpot: Option<i32>,
        dice_roll: Option<i32>
    }
);

#[derive(Default)]
pub struct ServerConfigFilter {
    page: Option<u64>,
    page_size: Option<u64>,
}

#[async_trait::async_trait]
impl ApplyQueryFilter<entities::server_config::Entity> for ServerConfigFilter {
    async fn apply(
        &self,
        query: Select<entities::server_config::Entity>,
    ) -> Select<entities::server_config::Entity> {
        query
    }

    fn page_size(&self) -> u64 {
        self.page_size.unwrap_or(20)
    }

    fn page(&self) -> u64 {
        self.page.unwrap_or(1)
    }
}
