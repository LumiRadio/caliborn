use std::sync::Arc;

use caliborn::{
    DiscordOAuthClient, ServiceRegistry, build_oauth2_client, entities,
    liquidsoap::LiquidsoapClient, repositories::AlwaysCloneableConnection, services::UserId,
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

fn caliborn_test_sealer() -> Arc<caliborn::services::secrets::TokenSealer> {
    Arc::new(
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

async fn insert_user(conn: &AlwaysCloneableConnection, id: i64, boonbucks: i32, watched_time: i64) {
    entities::users::Entity::insert(entities::users::ActiveModel {
        id: ActiveValue::set(id),
        boonbucks: ActiveValue::set(boonbucks),
        watched_time: ActiveValue::set(watched_time),
        ..Default::default()
    })
    .exec(&**conn)
    .await
    .unwrap();
}

#[rstest]
#[awt]
#[tokio::test]
async fn profile_aggregates_balances_and_rank(
    #[future] db: (AlwaysCloneableConnection, ContainerAsync<Postgres>),
) {
    let (conn, _c) = db;
    // Three users with different listening times and balances.
    insert_user(&conn, 1, 100, 7200).await; // 2 hours
    insert_user(&conn, 2, 500, 18000).await; // 5 hours, top hours
    insert_user(&conn, 3, 1000, 3600).await; // 1 hour, top boonbucks

    let registry = build_registry(conn.clone());
    let p1 = registry
        .user_service()
        .get_profile(UserId::from(1_i64))
        .await
        .unwrap();
    assert_eq!(p1.id, 1);
    assert_eq!(p1.listening_hours, 2);
    assert_eq!(p1.position_by_hours, 2); // beaten by user 2
    assert_eq!(p1.position_by_boonbucks, 3); // beaten by 2 and 3
    assert_eq!(p1.cans_added, 0);
    assert_eq!(p1.role, "user");
    assert!(p1.permissions.contains(&"use_minigames".to_string()));

    let p2 = registry
        .user_service()
        .get_profile(UserId::from(2_i64))
        .await
        .unwrap();
    assert_eq!(p2.position_by_hours, 1);

    let p3 = registry
        .user_service()
        .get_profile(UserId::from(3_i64))
        .await
        .unwrap();
    assert_eq!(p3.position_by_boonbucks, 1);
}

#[rstest]
#[awt]
#[tokio::test]
async fn profile_includes_can_count(
    #[future] db: (AlwaysCloneableConnection, ContainerAsync<Postgres>),
) {
    let (conn, _c) = db;
    insert_user(&conn, 1, 0, 0).await;

    // Insert 4 cans, only 3 legit.
    for legit in [true, true, true, false] {
        entities::cans::Entity::insert(entities::cans::ActiveModel {
            added_by: ActiveValue::set(1),
            legit: ActiveValue::set(legit),
            ..Default::default()
        })
        .exec(&*conn)
        .await
        .unwrap();
    }

    let registry = build_registry(conn.clone());
    let p = registry
        .user_service()
        .get_profile(UserId::from(1_i64))
        .await
        .unwrap();
    assert_eq!(p.cans_added, 3);
}

#[rstest]
#[awt]
#[tokio::test]
async fn profile_auto_creates_missing_user(
    #[future] db: (AlwaysCloneableConnection, ContainerAsync<Postgres>),
) {
    let (conn, _c) = db;
    let registry = build_registry(conn.clone());
    let p = registry
        .user_service()
        .get_profile(UserId::from(42_i64))
        .await
        .unwrap();
    assert_eq!(p.id, 42);
    assert_eq!(p.boonbucks, 0);
    assert_eq!(p.cans_added, 0);
}
