//! Tests for the YouTube-via-Discord-Connections sync path. The Discord HTTP
//! call itself is not exercised here (it would need a mock OAuth server) —
//! instead we verify the persistence side: the repo upsert.

use caliborn::{
    entities,
    repositories::{AlwaysCloneableConnection, BaseRepository, users::UserRepositoryExt},
};
use migration::MigratorTrait;
use rstest::{fixture, rstest};
use sea_orm::{ActiveValue, DatabaseConnection, EntityTrait, PaginatorTrait};
use testcontainers::{ContainerAsync, ImageExt, runners::AsyncRunner};
use testcontainers_modules::postgres::Postgres;

#[fixture]
async fn db() -> (AlwaysCloneableConnection, ContainerAsync<Postgres>) {
    let container = testcontainers_modules::postgres::Postgres::default()
        .with_tag("12")
        .start()
        .await
        .expect("Failed to start postgres container");

    let conn: DatabaseConnection = sea_orm::Database::connect(&format!(
        "postgres://postgres:postgres@{}:{}/postgres",
        container.get_host().await.unwrap(),
        container.get_host_port_ipv4(5432).await.unwrap()
    ))
    .await
    .expect("Failed to connect to postgres");

    migration::Migrator::up(&conn, None)
        .await
        .expect("Failed to run migrations");

    (AlwaysCloneableConnection::from(conn), container)
}

async fn insert_user(conn: &AlwaysCloneableConnection, id: i64) {
    entities::users::Entity::insert(entities::users::ActiveModel {
        id: ActiveValue::set(id),
        ..Default::default()
    })
    .exec(&**conn)
    .await
    .unwrap();
}

#[rstest]
#[awt]
#[tokio::test]
async fn upsert_inserts_when_absent(
    #[future] db: (AlwaysCloneableConnection, ContainerAsync<Postgres>),
) {
    let (conn, _c) = db;
    insert_user(&conn, 1).await;

    let repo: BaseRepository<entities::users::Entity> = BaseRepository::new(&conn);
    repo.upsert_youtube_account(1, "UCxxxx", "Channel One")
        .await
        .unwrap();

    let count = entities::connected_youtube_accounts::Entity::find()
        .count(&*conn)
        .await
        .unwrap();
    assert_eq!(count, 1);
}

#[rstest]
#[awt]
#[tokio::test]
async fn upsert_is_idempotent(#[future] db: (AlwaysCloneableConnection, ContainerAsync<Postgres>)) {
    let (conn, _c) = db;
    insert_user(&conn, 1).await;

    let repo: BaseRepository<entities::users::Entity> = BaseRepository::new(&conn);
    repo.upsert_youtube_account(1, "UCxxxx", "Channel One")
        .await
        .unwrap();
    repo.upsert_youtube_account(1, "UCxxxx", "Channel One")
        .await
        .unwrap();

    let count = entities::connected_youtube_accounts::Entity::find()
        .count(&*conn)
        .await
        .unwrap();
    assert_eq!(count, 1);
}

#[rstest]
#[awt]
#[tokio::test]
async fn upsert_refreshes_display_name(
    #[future] db: (AlwaysCloneableConnection, ContainerAsync<Postgres>),
) {
    use sea_orm::{ColumnTrait, QueryFilter};

    let (conn, _c) = db;
    insert_user(&conn, 1).await;

    let repo: BaseRepository<entities::users::Entity> = BaseRepository::new(&conn);
    repo.upsert_youtube_account(1, "UCxxxx", "Old Name")
        .await
        .unwrap();
    repo.upsert_youtube_account(1, "UCxxxx", "New Name")
        .await
        .unwrap();

    let model = entities::connected_youtube_accounts::Entity::find()
        .filter(entities::connected_youtube_accounts::Column::UserId.eq(1_i64))
        .filter(entities::connected_youtube_accounts::Column::YoutubeChannelId.eq("UCxxxx"))
        .one(&*conn)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(model.youtube_channel_name, "New Name");

    let count = entities::connected_youtube_accounts::Entity::find()
        .count(&*conn)
        .await
        .unwrap();
    assert_eq!(count, 1);
}

#[rstest]
#[awt]
#[tokio::test]
async fn upsert_keeps_other_channels(
    #[future] db: (AlwaysCloneableConnection, ContainerAsync<Postgres>),
) {
    let (conn, _c) = db;
    insert_user(&conn, 1).await;

    let repo: BaseRepository<entities::users::Entity> = BaseRepository::new(&conn);
    repo.upsert_youtube_account(1, "UCaaaa", "First")
        .await
        .unwrap();
    repo.upsert_youtube_account(1, "UCbbbb", "Second")
        .await
        .unwrap();

    let count = entities::connected_youtube_accounts::Entity::find()
        .count(&*conn)
        .await
        .unwrap();
    assert_eq!(count, 2);
}
