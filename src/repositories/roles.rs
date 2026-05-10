use sea_orm::{ColumnTrait, QueryFilter, Select};

use crate::{entities, generate_dtos, repositories::ApplyQueryFilter};

generate_dtos!(
    entities::roles::Entity,
    CreateRoleDto {
        name: String,
        description: String,
    },
    UpdateRoleDto {
        description: Option<String>,
    }
);

#[derive(Default)]
pub struct RoleFilter {
    name: Option<String>,
    page: Option<u64>,
    page_size: Option<u64>,
}

#[async_trait::async_trait]
impl ApplyQueryFilter<entities::roles::Entity> for RoleFilter {
    async fn apply(
        &self,
        query: Select<entities::roles::Entity>,
    ) -> Select<entities::roles::Entity> {
        let mut query = query;

        if let Some(name) = &self.name {
            query = query.filter(entities::roles::Column::Name.eq(name));
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
