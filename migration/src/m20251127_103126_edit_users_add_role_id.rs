use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Users::Table)
                    .add_column(string(Users::Role).default("user"))
                    .add_foreign_key(
                        ForeignKey::create()
                            .from(Users::Table, Users::Role)
                            .to(Roles::Table, Roles::Name)
                            .on_delete(ForeignKeyAction::SetDefault)
                            .get_foreign_key(),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Users::Table)
                    .drop_column(Users::Role)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum Roles {
    Table,
    Name,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Role,
}
