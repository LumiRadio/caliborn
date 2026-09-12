use pg_fts::gin_index;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_index(gin_index(
                SongsFulltextTsvectorIdx,
                SongsFulltext::Table,
                SongsFulltext::Tsvector,
            ))
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name(SongsFulltextTsvectorIdx.to_string())
                    .table(SongsFulltext::Table)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
struct SongsFulltextTsvectorIdx;

#[derive(DeriveIden)]
enum SongsFulltext {
    Table,
    Tsvector,
}
