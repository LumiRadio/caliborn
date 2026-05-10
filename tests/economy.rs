use std::sync::Arc;

use caliborn::{
    DiscordOAuthClient, ServiceRegistry, build_oauth2_client, entities,
    liquidsoap::LiquidsoapClient,
    repositories::{
        AlwaysCloneableConnection, BaseRepository,
        users::{TransferError, UserRepositoryExt},
    },
    services::{UserId, economy::EconomyServiceError},
};
use hmac::{Hmac, Mac};
use migration::MigratorTrait;
use rstest::{fixture, rstest};
use sea_orm::{ActiveValue, DatabaseConnection, EntityTrait};
use sha2::Sha256;
use testcontainers::{ContainerAsync, ImageExt, runners::AsyncRunner};
use testcontainers_modules::postgres::Postgres;
use tokio::sync::Mutex;

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

mockall::mock! {
    pub LiquidsoapClient {}

    #[async_trait::async_trait]
    impl LiquidsoapClient for LiquidsoapClient {
        async fn command(&mut self, cmd: &str) -> Result<String, caliborn::LiquidsoapError>;
        async fn command_with_reconnect(&mut self, cmd: &str) -> Result<String, caliborn::LiquidsoapError>;
        async fn shutdown(mut self) -> Result<(), caliborn::LiquidsoapError>;
    }
}

fn caliborn_test_sealer() -> std::sync::Arc<caliborn::services::secrets::TokenSealer> {
    std::sync::Arc::new(
        caliborn::services::secrets::TokenSealer::from_hex(
            "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff",
        )
        .unwrap(),
    )
}

fn build_registry(conn: AlwaysCloneableConnection) -> ServiceRegistry {
    let jwt = Hmac::<Sha256>::new_from_slice(b"jwt").unwrap();
    let hmac = Hmac::<Sha256>::new_from_slice(b"hmac").unwrap();
    let oauth: DiscordOAuthClient =
        build_oauth2_client("", "", "http://localhost:8080/callback").unwrap();
    let ls = Arc::new(Mutex::new(MockLiquidsoapClient::new())) as Arc<Mutex<dyn LiquidsoapClient>>;
    ServiceRegistry::new(
        conn,
        jwt,
        hmac,
        oauth,
        ls,
        caliborn::RealtimeBroadcaster::new(),
        "test_app_id".to_string(),
        "LumiRadio".to_string(),
        caliborn_test_sealer(),
    )
}

async fn insert_user(conn: &AlwaysCloneableConnection, id: i64, boonbucks: i32) {
    entities::users::Entity::insert(entities::users::ActiveModel {
        id: ActiveValue::set(id),
        boonbucks: ActiveValue::set(boonbucks),
        ..Default::default()
    })
    .exec(&**conn)
    .await
    .unwrap();
}

async fn balance(conn: &AlwaysCloneableConnection, id: i64) -> i32 {
    entities::users::Entity::find_by_id(id)
        .one(&**conn)
        .await
        .unwrap()
        .unwrap()
        .boonbucks
}

#[rstest]
#[awt]
#[tokio::test]
async fn transfer_succeeds_and_returns_balances(
    #[future] db: (AlwaysCloneableConnection, ContainerAsync<Postgres>),
) {
    let (conn, _c) = db;
    insert_user(&conn, 1, 100).await;
    insert_user(&conn, 2, 50).await;

    let repo: BaseRepository<entities::users::Entity> = BaseRepository::new(&conn);
    let (sender, recipient) = repo.transfer_boonbucks(1, 2, 30).await.unwrap();

    assert_eq!(sender, 70);
    assert_eq!(recipient, 80);
    assert_eq!(balance(&conn, 1).await, 70);
    assert_eq!(balance(&conn, 2).await, 80);
}

#[rstest]
#[awt]
#[tokio::test]
async fn transfer_rejects_insufficient_funds(
    #[future] db: (AlwaysCloneableConnection, ContainerAsync<Postgres>),
) {
    let (conn, _c) = db;
    insert_user(&conn, 1, 10).await;
    insert_user(&conn, 2, 0).await;

    let repo: BaseRepository<entities::users::Entity> = BaseRepository::new(&conn);
    let err = repo.transfer_boonbucks(1, 2, 50).await.unwrap_err();

    assert!(matches!(err, TransferError::InsufficientFunds));
    assert_eq!(balance(&conn, 1).await, 10);
    assert_eq!(balance(&conn, 2).await, 0);
}

#[rstest]
#[awt]
#[tokio::test]
async fn transfer_rejects_missing_sender(
    #[future] db: (AlwaysCloneableConnection, ContainerAsync<Postgres>),
) {
    let (conn, _c) = db;
    insert_user(&conn, 2, 0).await;

    let repo: BaseRepository<entities::users::Entity> = BaseRepository::new(&conn);
    let err = repo.transfer_boonbucks(999, 2, 10).await.unwrap_err();

    assert!(matches!(err, TransferError::SenderNotFound));
}

#[rstest]
#[awt]
#[tokio::test]
async fn transfer_rejects_missing_recipient(
    #[future] db: (AlwaysCloneableConnection, ContainerAsync<Postgres>),
) {
    let (conn, _c) = db;
    insert_user(&conn, 1, 100).await;

    let repo: BaseRepository<entities::users::Entity> = BaseRepository::new(&conn);
    let err = repo.transfer_boonbucks(1, 999, 10).await.unwrap_err();

    assert!(matches!(err, TransferError::RecipientNotFound));
    // Sender must be rolled back to original 100 because the second UPDATE failed.
    assert_eq!(balance(&conn, 1).await, 100);
}

#[rstest]
#[awt]
#[tokio::test]
async fn pay_rejects_self_transfer(
    #[future] db: (AlwaysCloneableConnection, ContainerAsync<Postgres>),
) {
    let (conn, _c) = db;
    insert_user(&conn, 1, 100).await;

    let registry = build_registry(conn.clone());
    let economy = registry.economy_service();
    let err = economy
        .pay(UserId::from(1_i64), UserId::from(1_i64), 10)
        .await
        .unwrap_err();

    assert!(matches!(err, EconomyServiceError::SelfTransfer));
    assert_eq!(balance(&conn, 1).await, 100);
}

#[rstest]
#[awt]
#[tokio::test]
async fn pay_rejects_invalid_amount(
    #[future] db: (AlwaysCloneableConnection, ContainerAsync<Postgres>),
) {
    let (conn, _c) = db;
    insert_user(&conn, 1, 100).await;
    insert_user(&conn, 2, 0).await;

    let registry = build_registry(conn.clone());
    let economy = registry.economy_service();
    let err = economy
        .pay(UserId::from(1_i64), UserId::from(2_i64), 0)
        .await
        .unwrap_err();

    assert!(matches!(err, EconomyServiceError::InvalidAmount));
    assert_eq!(balance(&conn, 1).await, 100);
    assert_eq!(balance(&conn, 2).await, 0);
}
