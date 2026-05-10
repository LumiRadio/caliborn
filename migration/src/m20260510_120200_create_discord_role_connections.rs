use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(DiscordRoleConnections::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(DiscordRoleConnections::UserId)
                            .big_integer()
                            .not_null()
                            .primary_key(),
                    )
                    .col(timestamp_null(DiscordRoleConnections::LastPushedAt))
                    .col(integer_null(DiscordRoleConnections::ListeningHoursSnapshot))
                    .col(integer_null(DiscordRoleConnections::CanCountSnapshot))
                    .col(integer_null(DiscordRoleConnections::BoonbucksSnapshot))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-discord_role_connections-user_id")
                            .from(
                                DiscordRoleConnections::Table,
                                DiscordRoleConnections::UserId,
                            )
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(DiscordRoleConnections::Table)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum DiscordRoleConnections {
    Table,
    UserId,
    LastPushedAt,
    ListeningHoursSnapshot,
    CanCountSnapshot,
    BoonbucksSnapshot,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}
