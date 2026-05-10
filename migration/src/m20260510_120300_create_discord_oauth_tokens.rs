use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(DiscordOauthTokens::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(DiscordOauthTokens::UserId)
                            .big_integer()
                            .not_null()
                            .primary_key(),
                    )
                    .col(binary(DiscordOauthTokens::AccessTokenCiphertext))
                    .col(binary(DiscordOauthTokens::AccessTokenNonce))
                    .col(binary(DiscordOauthTokens::RefreshTokenCiphertext))
                    .col(binary(DiscordOauthTokens::RefreshTokenNonce))
                    .col(timestamp(DiscordOauthTokens::AccessExpiresAt))
                    .col(text(DiscordOauthTokens::Scopes))
                    .col(
                        ColumnDef::new(DiscordOauthTokens::UpdatedAt)
                            .timestamp()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-discord_oauth_tokens-user_id")
                            .from(DiscordOauthTokens::Table, DiscordOauthTokens::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(DiscordOauthTokens::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum DiscordOauthTokens {
    Table,
    UserId,
    AccessTokenCiphertext,
    AccessTokenNonce,
    RefreshTokenCiphertext,
    RefreshTokenNonce,
    AccessExpiresAt,
    Scopes,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}
