use sea_orm_migration::{prelude::*, schema::*, sea_orm::Statement};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        let (sql, values) = sea_query::Query::insert()
            .into_table(Roles::Table)
            .columns([Roles::Name, Roles::Description])
            .values_panic([
                "admin".into(),
                "Administrator role - can do everything".into(),
            ])
            .values_panic([
                "moderator".into(),
                "Moderator role - can moderate the chat and manage some commands".into(),
            ])
            .values_panic([
                "user".into(),
                "User role - can use the chat and some commands".into(),
            ])
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
enum Roles {
    Table,
    Name,
    Description,
}
