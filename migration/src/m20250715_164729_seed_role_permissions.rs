use sea_orm_migration::{prelude::*, schema::*, sea_orm::Statement};
use shared_constants::permissions::{
    Permission, PERM_MANAGE_ACTIVITY_ROLES, PERM_MANAGE_STREAM, PERM_MANAGE_USERS, PERM_USE_BOT,
    PERM_USE_MINIGAMES, PERM_USE_WEB_CHAT,
};

#[derive(DeriveMigrationName)]
pub struct Migration;

fn query_role_permission(role_name: &str, permissions: &[Permission]) -> InsertStatement {
    let mut query = Query::insert()
        .into_table(RolePermissions::Table)
        .columns([RolePermissions::Role, RolePermissions::Permission])
        .to_owned();

    for permission in permissions {
        query.values_panic([role_name.into(), permission.name.into()]);
    }

    query.to_owned()
}

async fn set_role_permissions(
    db: &SchemaManagerConnection<'_>,
    role_name: &str,
    permissions: &[Permission],
) -> Result<(), DbErr> {
    let (sql, values) = query_role_permission(role_name, permissions).build(PostgresQueryBuilder);
    db.execute(Statement::from_sql_and_values(
        db.get_database_backend(),
        sql,
        values,
    ))
    .await?;
    Ok(())
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        set_role_permissions(
            db,
            "admin",
            &[
                PERM_MANAGE_USERS,
                PERM_USE_MINIGAMES,
                PERM_USE_WEB_CHAT,
                PERM_USE_BOT,
                PERM_MANAGE_STREAM,
                PERM_MANAGE_ACTIVITY_ROLES,
            ],
        )
        .await?;

        set_role_permissions(
            db,
            "moderator",
            &[
                PERM_MANAGE_USERS,
                PERM_USE_MINIGAMES,
                PERM_USE_WEB_CHAT,
                PERM_USE_BOT,
                PERM_MANAGE_ACTIVITY_ROLES,
            ],
        )
        .await?;

        set_role_permissions(
            db,
            "user",
            &[PERM_USE_MINIGAMES, PERM_USE_WEB_CHAT, PERM_USE_BOT],
        )
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}

#[derive(DeriveIden)]
enum RolePermissions {
    Table,
    Role,
    Permission,
}
