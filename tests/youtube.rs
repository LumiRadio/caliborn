//! Tests for the YouTube-account upsert (Discord-connections sync persistence).
//! Uses the shared `ScenarioEnv` harness (see `common.rs`).

mod common;
use common::*;

use caliborn::{
    entities,
    repositories::{BaseRepository, users::UserRepositoryExt},
};
use rstest::rstest;
use sea_orm::{EntityTrait, PaginatorTrait};

#[rstest]
#[awt]
#[tokio::test]
async fn upsert_inserts_when_absent(#[future] scenario: ScenarioEnv) {
    let env = scenario;
    env.insert_user(1, 0, 0).await;

    let repo: BaseRepository<entities::users::Entity> = BaseRepository::new(&env.conn);
    repo.upsert_youtube_account(1, "UCxxxx", "Channel One")
        .await
        .unwrap();

    let count = entities::connected_youtube_accounts::Entity::find()
        .count(&*env.conn)
        .await
        .unwrap();
    assert_eq!(count, 1);
}

#[rstest]
#[awt]
#[tokio::test]
async fn upsert_is_idempotent(#[future] scenario: ScenarioEnv) {
    let env = scenario;
    env.insert_user(1, 0, 0).await;

    let repo: BaseRepository<entities::users::Entity> = BaseRepository::new(&env.conn);
    repo.upsert_youtube_account(1, "UCxxxx", "Channel One")
        .await
        .unwrap();
    repo.upsert_youtube_account(1, "UCxxxx", "Channel One")
        .await
        .unwrap();

    let count = entities::connected_youtube_accounts::Entity::find()
        .count(&*env.conn)
        .await
        .unwrap();
    assert_eq!(count, 1);
}

#[rstest]
#[awt]
#[tokio::test]
async fn upsert_refreshes_display_name(#[future] scenario: ScenarioEnv) {
    use sea_orm::{ColumnTrait, QueryFilter};

    let env = scenario;
    env.insert_user(1, 0, 0).await;

    let repo: BaseRepository<entities::users::Entity> = BaseRepository::new(&env.conn);
    repo.upsert_youtube_account(1, "UCxxxx", "Old Name")
        .await
        .unwrap();
    repo.upsert_youtube_account(1, "UCxxxx", "New Name")
        .await
        .unwrap();

    let model = entities::connected_youtube_accounts::Entity::find()
        .filter(entities::connected_youtube_accounts::Column::UserId.eq(1_i64))
        .filter(entities::connected_youtube_accounts::Column::YoutubeChannelId.eq("UCxxxx"))
        .one(&*env.conn)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(model.youtube_channel_name, "New Name");

    let count = entities::connected_youtube_accounts::Entity::find()
        .count(&*env.conn)
        .await
        .unwrap();
    assert_eq!(count, 1);
}

#[rstest]
#[awt]
#[tokio::test]
async fn upsert_keeps_other_channels(#[future] scenario: ScenarioEnv) {
    let env = scenario;
    env.insert_user(1, 0, 0).await;

    let repo: BaseRepository<entities::users::Entity> = BaseRepository::new(&env.conn);
    repo.upsert_youtube_account(1, "UCaaaa", "First")
        .await
        .unwrap();
    repo.upsert_youtube_account(1, "UCbbbb", "Second")
        .await
        .unwrap();

    let count = entities::connected_youtube_accounts::Entity::find()
        .count(&*env.conn)
        .await
        .unwrap();
    assert_eq!(count, 2);
}
