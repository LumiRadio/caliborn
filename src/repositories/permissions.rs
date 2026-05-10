use sea_orm::{
    ActiveValue, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, Select, Statement,
};
use sea_query::{Expr, OnConflict, Query, UnionType};
use shared_constants::permissions::Permission;

use crate::{
    RepositoryError, entities, generate_dtos,
    repositories::{ApplyQueryFilter, BaseRepository},
    sea_orm_utils::BoxedQueryBuilder,
};

generate_dtos!(
    entities::role_permissions::Entity,
    CreateRolePermissionDto {
        role: String,
        permission: String,
    },
    UpdateRolePermissionDto {
        permission: Option<String>,
    }
);

generate_dtos!(
    entities::user_permissions::Entity,
    CreateUserPermissionDto {
        user_id: i64,
        permission: String,
    },
    UpdateUserPermissionDto {
        permission: Option<String>,
    }
);

#[derive(Default)]
pub struct RolePermissionFilter {
    role: Option<String>,
    page: Option<u64>,
    page_size: Option<u64>,
}

#[async_trait::async_trait]
impl ApplyQueryFilter<entities::role_permissions::Entity> for RolePermissionFilter {
    async fn apply(
        &self,
        query: Select<entities::role_permissions::Entity>,
    ) -> Select<entities::role_permissions::Entity> {
        let mut query = query;

        if let Some(role) = &self.role {
            query = query.filter(entities::role_permissions::Column::Role.eq(role));
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

#[derive(Default)]
pub struct UserPermissionFilter {
    user_id: Option<i64>,
    page: Option<u64>,
    page_size: Option<u64>,
}

#[async_trait::async_trait]
impl ApplyQueryFilter<entities::user_permissions::Entity> for UserPermissionFilter {
    async fn apply(
        &self,
        query: Select<entities::user_permissions::Entity>,
    ) -> Select<entities::user_permissions::Entity> {
        let mut query = query;

        if let Some(user_id) = self.user_id {
            query = query.filter(entities::user_permissions::Column::UserId.eq(user_id));
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
pub trait RolePermissionRepositoryExt: Send + Sync + 'static {
    async fn set_role_permissions(
        &self,
        role: &str,
        permissions: &[Permission],
    ) -> Result<(), RepositoryError>;
}

#[async_trait::async_trait]
pub trait UserPermissionRepositoryExt: Send + Sync + 'static {
    async fn set_user_permissions(
        &self,
        user_id: i64,
        to_grant: &[Permission],
        to_revoke: &[Permission],
    ) -> Result<(), RepositoryError>;
    async fn get_effective_permissions(
        &self,
        user_id: i64,
    ) -> Result<Vec<Permission>, RepositoryError>;
}

#[async_trait::async_trait]
impl UserPermissionRepositoryExt for BaseRepository<entities::user_permissions::Entity> {
    async fn set_user_permissions(
        &self,
        user_id: i64,
        to_grant: &[Permission],
        to_revoke: &[Permission],
    ) -> Result<(), RepositoryError> {
        let mut to_insert = to_grant
            .iter()
            .map(|p| entities::user_permissions::ActiveModel {
                user_id: ActiveValue::set(user_id),
                permission: ActiveValue::set(p.name.to_string()),
                granted: ActiveValue::set(true),
            })
            .collect::<Vec<_>>();

        to_insert.extend(
            to_revoke
                .iter()
                .map(|p| entities::user_permissions::ActiveModel {
                    user_id: ActiveValue::set(user_id),
                    permission: ActiveValue::set(p.name.to_string()),
                    granted: ActiveValue::set(false),
                })
                .collect::<Vec<_>>(),
        );

        entities::user_permissions::Entity::insert_many(to_insert)
            .on_conflict(
                OnConflict::columns([
                    entities::user_permissions::Column::UserId,
                    entities::user_permissions::Column::Permission,
                ])
                .do_nothing()
                .to_owned(),
            )
            .exec(&self.db)
            .await?;

        Ok(())
    }

    async fn get_effective_permissions(
        &self,
        user_id: i64,
    ) -> Result<Vec<Permission>, RepositoryError> {
        // i dont think seaorm can do this...
        // but maybe sea_query can
        let granted_query = Query::select()
            .column((
                entities::user_permissions::Entity,
                entities::user_permissions::Column::Permission,
            ))
            .from(entities::user_permissions::Entity)
            .and_where(
                Expr::col((
                    entities::user_permissions::Entity,
                    entities::user_permissions::Column::UserId,
                ))
                .eq(user_id),
            )
            .and_where(
                Expr::col((
                    entities::user_permissions::Entity,
                    entities::user_permissions::Column::Granted,
                ))
                .eq(true),
            )
            .to_owned();

        let revoked_query = Query::select()
            .column((
                entities::user_permissions::Entity,
                entities::user_permissions::Column::Permission,
            ))
            .from(entities::user_permissions::Entity)
            .and_where(
                Expr::col((
                    entities::user_permissions::Entity,
                    entities::user_permissions::Column::UserId,
                ))
                .eq(user_id),
            )
            .and_where(
                Expr::col((
                    entities::user_permissions::Entity,
                    entities::user_permissions::Column::Granted,
                ))
                .eq(false),
            )
            .to_owned();

        let query = sea_query::Query::select()
            .distinct()
            .column((
                entities::permissions::Entity,
                entities::permissions::Column::Name,
            ))
            .from(entities::users::Entity)
            .join(
                sea_orm::JoinType::InnerJoin,
                entities::role_permissions::Entity,
                Expr::col((entities::users::Entity, entities::users::Column::Role)).eq(Expr::col(
                    (
                        entities::role_permissions::Entity,
                        entities::role_permissions::Column::Role,
                    ),
                )),
            )
            .join(
                sea_orm::JoinType::InnerJoin,
                entities::permissions::Entity,
                Expr::col((
                    entities::role_permissions::Entity,
                    entities::role_permissions::Column::Permission,
                ))
                .eq(Expr::col((
                    entities::permissions::Entity,
                    entities::permissions::Column::Name,
                ))),
            )
            .and_where(
                Expr::col((entities::users::Entity, entities::users::Column::Id)).eq(user_id),
            )
            .union(UnionType::Distinct, granted_query)
            .union(UnionType::Except, revoked_query)
            .to_owned();

        let (sql, values) = query.build(BoxedQueryBuilder(
            self.db.get_database_backend().get_query_builder(),
        ));

        let result = self
            .db
            .query_all(Statement::from_sql_and_values(
                self.db.get_database_backend(),
                sql,
                values,
            ))
            .await?;

        Ok(result
            .into_iter()
            .map(|r| Permission::from_name(&r.try_get_by_index::<String>(0).unwrap()).unwrap())
            .collect())
    }
}

#[async_trait::async_trait]
impl RolePermissionRepositoryExt for BaseRepository<entities::role_permissions::Entity> {
    async fn set_role_permissions(
        &self,
        role: &str,
        permissions: &[Permission],
    ) -> Result<(), RepositoryError> {
        let to_insert = permissions
            .iter()
            .map(|p| entities::role_permissions::ActiveModel {
                role: ActiveValue::set(role.to_string()),
                permission: ActiveValue::set(p.name.to_string()),
                ..Default::default()
            })
            .collect::<Vec<_>>();

        entities::role_permissions::Entity::insert_many(to_insert)
            .on_conflict(
                OnConflict::columns([
                    entities::role_permissions::Column::Role,
                    entities::role_permissions::Column::Permission,
                ])
                .do_nothing()
                .to_owned(),
            )
            .exec(&self.db)
            .await?;

        Ok(())
    }
}
