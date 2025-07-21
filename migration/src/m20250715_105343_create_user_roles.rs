use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(UserRoles::Table)
                    .if_not_exists()
                    .col(big_integer(UserRoles::UserId))
                    .foreign_key(
                        ForeignKey::create()
                            .from(UserRoles::Table, UserRoles::UserId)
                            .to(Users::Table, Users::Id),
                    )
                    .col(string(UserRoles::Role))
                    .foreign_key(
                        ForeignKey::create()
                            .from(UserRoles::Table, UserRoles::Role)
                            .to(Roles::Table, Roles::Name),
                    )
                    .primary_key(Index::create().col(UserRoles::UserId).col(UserRoles::Role))
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(UserRoles::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum Roles {
    Table,
    Name,
}

#[derive(DeriveIden)]
enum UserRoles {
    Table,
    UserId,
    Role,
}
