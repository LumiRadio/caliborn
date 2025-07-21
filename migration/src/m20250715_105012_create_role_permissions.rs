use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(RolePermissions::Table)
                    .if_not_exists()
                    .col(string(RolePermissions::Role))
                    .foreign_key(
                        ForeignKey::create()
                            .from(RolePermissions::Table, RolePermissions::Role)
                            .to(Roles::Table, Roles::Name),
                    )
                    .col(string(RolePermissions::Permission))
                    .foreign_key(
                        ForeignKey::create()
                            .from(RolePermissions::Table, RolePermissions::Permission)
                            .to(Permissions::Table, Permissions::Name),
                    )
                    .primary_key(
                        Index::create()
                            .col(RolePermissions::Role)
                            .col(RolePermissions::Permission),
                    )
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(RolePermissions::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Roles {
    Table,
    Name,
}

#[derive(DeriveIden)]
enum Permissions {
    Table,
    Name,
}

#[derive(DeriveIden)]
enum RolePermissions {
    Table,
    Role,
    Permission,
}
