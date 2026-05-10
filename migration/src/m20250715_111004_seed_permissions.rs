use sea_orm_migration::{prelude::*, schema::*, sea_orm::Statement};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        let (sql, values) = sea_query::Query::insert()
            .into_table(Permissions::Table)
            .columns([
                Permissions::Name,
                Permissions::Description,
                Permissions::BuiltIn,
            ])
            .values_panic(shared_constants::permissions::PERM_MANAGE_USERS)
            .values_panic(shared_constants::permissions::PERM_USE_MINIGAMES)
            .values_panic(shared_constants::permissions::PERM_USE_WEB_CHAT)
            .values_panic(shared_constants::permissions::PERM_USE_BOT)
            .values_panic(shared_constants::permissions::PERM_MANAGE_STREAM)
            .values_panic(shared_constants::permissions::PERM_MANAGE_ACTIVITY_ROLES)
            .build(PostgresQueryBuilder);

        db.execute(Statement::from_sql_and_values(
            db.get_database_backend(),
            sql,
            values,
        ))
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}

#[derive(DeriveIden)]
enum Permissions {
    Table,
    Name,
    Description,
    BuiltIn,
}
