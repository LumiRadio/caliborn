use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(RadioState::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(RadioState::Id)
                            .small_integer()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(RadioState::SlotJackpot)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(RadioState::DiceRollTarget)
                            .integer()
                            .not_null()
                            .default(111),
                    )
                    .col(
                        ColumnDef::new(RadioState::DiceRollMode)
                            .small_integer()
                            .not_null()
                            .default(3),
                    )
                    .col(
                        ColumnDef::new(RadioState::UpdatedAt)
                            .timestamp()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .check(Expr::col(RadioState::Id).eq(1))
                    .to_owned(),
            )
            .await?;

        let db = manager.get_connection();
        db.execute_unprepared(
            r#"
            INSERT INTO radio_state (id, slot_jackpot, dice_roll_target, dice_roll_mode, updated_at)
            SELECT
                1,
                COALESCE(MAX(slot_jackpot), 0)::bigint,
                COALESCE(MAX(dice_roll), 111),
                CASE WHEN COALESCE(MAX(dice_roll), 111) >= 1000 THEN 4 ELSE 3 END,
                CURRENT_TIMESTAMP
            FROM server_config
            ON CONFLICT (id) DO NOTHING;
            "#,
        )
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(RadioState::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum RadioState {
    Table,
    Id,
    SlotJackpot,
    DiceRollTarget,
    DiceRollMode,
    UpdatedAt,
}
