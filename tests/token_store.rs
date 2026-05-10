//! Tests for `TokenStore` persistence. Refresh path requires a live Discord
//! token endpoint and is not exercised here.

use std::sync::Arc;

use caliborn::{
    DiscordOAuthClient, build_oauth2_client, entities,
    repositories::AlwaysCloneableConnection,
    services::{discord_oauth_tokens::TokenStore, secrets::TokenSealer},
};
use migration::MigratorTrait;
use rstest::{fixture, rstest};
use sea_orm::{ActiveValue, DatabaseConnection, EntityTrait};
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

fn make_store(conn: AlwaysCloneableConnection) -> TokenStore {
    let sealer = Arc::new(
        TokenSealer::from_hex("00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff")
            .unwrap(),
    );
    let oauth: DiscordOAuthClient =
        build_oauth2_client("", "", "http://localhost:8080/callback").unwrap();
    let http_client = reqwest::Client::new();
    TokenStore::new(conn, sealer, oauth, http_client)
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
async fn store_and_fetch_roundtrips(
    #[future] db: (AlwaysCloneableConnection, ContainerAsync<Postgres>),
) {
    let (conn, _c) = db;
    insert_user(&conn, 1).await;

    let store = make_store(conn.clone());
    let expires = chrono::Utc::now().naive_utc() + chrono::Duration::hours(1);
    store
        .store(1, "access-1", "refresh-1", expires, "identify connections")
        .await
        .unwrap();

    let got = store.fetch(1).await.unwrap();
    assert_eq!(got.access_token, "access-1");
    assert_eq!(got.refresh_token, "refresh-1");
    assert_eq!(got.scopes, "identify connections");
}

#[rstest]
#[awt]
#[tokio::test]
async fn store_overwrites_existing_row(
    #[future] db: (AlwaysCloneableConnection, ContainerAsync<Postgres>),
) {
    let (conn, _c) = db;
    insert_user(&conn, 1).await;

    let store = make_store(conn.clone());
    let expires = chrono::Utc::now().naive_utc() + chrono::Duration::hours(1);
    store.store(1, "a1", "r1", expires, "scope1").await.unwrap();
    store.store(1, "a2", "r2", expires, "scope2").await.unwrap();

    let got = store.fetch(1).await.unwrap();
    assert_eq!(got.access_token, "a2");
    assert_eq!(got.refresh_token, "r2");
    assert_eq!(got.scopes, "scope2");
}

#[rstest]
#[awt]
#[tokio::test]
async fn fetch_returns_not_found_when_absent(
    #[future] db: (AlwaysCloneableConnection, ContainerAsync<Postgres>),
) {
    use caliborn::services::discord_oauth_tokens::TokenStoreError;
    let (conn, _c) = db;

    let store = make_store(conn);
    let err = store.fetch(999).await.unwrap_err();
    assert!(matches!(err, TokenStoreError::NotFound(999)));
}

#[rstest]
#[awt]
#[tokio::test]
async fn valid_access_token_returns_stored_when_unexpired(
    #[future] db: (AlwaysCloneableConnection, ContainerAsync<Postgres>),
) {
    let (conn, _c) = db;
    insert_user(&conn, 1).await;

    let store = make_store(conn.clone());
    let expires = chrono::Utc::now().naive_utc() + chrono::Duration::hours(1);
    store
        .store(1, "still-valid", "rt", expires, "identify")
        .await
        .unwrap();

    let token = store.valid_access_token(1).await.unwrap();
    assert_eq!(token, "still-valid");
}
