use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(MinigameHistory::Table)
                    .if_not_exists()
                    .col(pk_auto(MinigameHistory::Id))
                    .col(big_integer(MinigameHistory::UserId))
                    .col(string_len(MinigameHistory::Game, 32))
                    .col(integer(MinigameHistory::Wager).default(0))
                    .col(integer(MinigameHistory::Payout).default(0))
                    .col(json_binary_null(MinigameHistory::Result))
                    .col(timestamp(MinigameHistory::PlayedAt).default(Expr::current_timestamp()))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-minigame_history-user_id")
                            .from(MinigameHistory::Table, MinigameHistory::UserId)
                            .to(Users::Table, Users::Id),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx-minigame_history-user_played")
                    .table(MinigameHistory::Table)
                    .col(MinigameHistory::UserId)
                    .col(MinigameHistory::PlayedAt)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(MinigameHistory::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum MinigameHistory {
    Table,
    Id,
    UserId,
    Game,
    Wager,
    Payout,
    Result,
    PlayedAt,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}
