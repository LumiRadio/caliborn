use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(UserPermissions::Table)
                    .if_not_exists()
                    .col(big_integer(UserPermissions::UserId))
                    .foreign_key(
                        ForeignKey::create()
                            .from(UserPermissions::Table, UserPermissions::UserId)
                            .to(Users::Table, Users::Id),
                    )
                    .col(string(UserPermissions::Permission))
                    .foreign_key(
                        ForeignKey::create()
                            .from(UserPermissions::Table, UserPermissions::Permission)
                            .to(Permissions::Table, Permissions::Name),
                    )
                    .col(boolean(UserPermissions::Granted))
                    .primary_key(
                        Index::create()
                            .col(UserPermissions::UserId)
                            .col(UserPermissions::Permission),
                    )
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(UserPermissions::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum UserPermissions {
    Table,
    UserId,
    Permission,
    Granted,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum Permissions {
    Table,
    Name,
}
