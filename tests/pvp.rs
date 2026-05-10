use std::sync::Arc;

use caliborn::{
    DiscordOAuthClient, ServiceRegistry, build_oauth2_client, entities,
    liquidsoap::LiquidsoapClient,
    repositories::AlwaysCloneableConnection,
    services::{UserId, minigames::pvp::PvpServiceError},
};
use hmac::{Hmac, Mac};
use migration::MigratorTrait;
use rstest::{fixture, rstest};
use sea_orm::{ActiveValue, DatabaseConnection, EntityTrait, PaginatorTrait};
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

fn build_registry(conn: AlwaysCloneableConnection) -> ServiceRegistry {
    let jwt = Hmac::<Sha256>::new_from_slice(b"jwt").unwrap();
    let hmac = Hmac::<Sha256>::new_from_slice(b"hmac").unwrap();
    let oauth: DiscordOAuthClient =
        build_oauth2_client("", "", "http://localhost:8080/callback").unwrap();
    let ls = Arc::new(Mutex::new(MockLiquidsoapClient::new())) as Arc<Mutex<dyn LiquidsoapClient>>;
    ServiceRegistry::new(conn, jwt, hmac, oauth, ls)
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
async fn challenge_resolves_and_transfers_bet(
    #[future] db: (AlwaysCloneableConnection, ContainerAsync<Postgres>),
) {
    let (conn, _c) = db;
    insert_user(&conn, 1, 100).await;
    insert_user(&conn, 2, 100).await;

    let registry = build_registry(conn.clone());
    let result = registry
        .minigame_service()
        .pvp
        .challenge(UserId::from(1_i64), UserId::from(2_i64))
        .await
        .unwrap();

    assert_eq!(result.transferred, 10);
    assert_eq!(result.challenger_id, 1);
    assert_eq!(result.opponent_id, 2);
    if result.challenger_won {
        assert_eq!(result.challenger_balance, 110);
        assert_eq!(result.opponent_balance, 90);
        assert_eq!(balance(&conn, 1).await, 110);
        assert_eq!(balance(&conn, 2).await, 90);
    } else {
        assert_eq!(result.challenger_balance, 90);
        assert_eq!(result.opponent_balance, 110);
        assert_eq!(balance(&conn, 1).await, 90);
        assert_eq!(balance(&conn, 2).await, 110);
    }

    let history_count = entities::minigame_history::Entity::find()
        .count(&*conn)
        .await
        .unwrap();
    assert_eq!(history_count, 1);
}

#[rstest]
#[awt]
#[tokio::test]
async fn challenge_rejects_self(
    #[future] db: (AlwaysCloneableConnection, ContainerAsync<Postgres>),
) {
    let (conn, _c) = db;
    insert_user(&conn, 1, 100).await;

    let registry = build_registry(conn.clone());
    let err = registry
        .minigame_service()
        .pvp
        .challenge(UserId::from(1_i64), UserId::from(1_i64))
        .await
        .unwrap_err();
    assert!(matches!(err, PvpServiceError::SelfChallenge));
    assert_eq!(balance(&conn, 1).await, 100);
}

#[rstest]
#[awt]
#[tokio::test]
async fn challenge_rejects_challenger_short_on_funds(
    #[future] db: (AlwaysCloneableConnection, ContainerAsync<Postgres>),
) {
    let (conn, _c) = db;
    insert_user(&conn, 1, 5).await;
    insert_user(&conn, 2, 100).await;

    let registry = build_registry(conn.clone());
    let err = registry
        .minigame_service()
        .pvp
        .challenge(UserId::from(1_i64), UserId::from(2_i64))
        .await
        .unwrap_err();
    assert!(matches!(err, PvpServiceError::ChallengerInsufficientFunds));
    assert_eq!(balance(&conn, 1).await, 5);
    assert_eq!(balance(&conn, 2).await, 100);
}

#[rstest]
#[awt]
#[tokio::test]
async fn challenge_rejects_opponent_short_on_funds(
    #[future] db: (AlwaysCloneableConnection, ContainerAsync<Postgres>),
) {
    let (conn, _c) = db;
    insert_user(&conn, 1, 100).await;
    insert_user(&conn, 2, 5).await;

    let registry = build_registry(conn.clone());
    let err = registry
        .minigame_service()
        .pvp
        .challenge(UserId::from(1_i64), UserId::from(2_i64))
        .await
        .unwrap_err();
    assert!(matches!(err, PvpServiceError::OpponentInsufficientFunds));
    assert_eq!(balance(&conn, 1).await, 100);
    assert_eq!(balance(&conn, 2).await, 5);
}

#[rstest]
#[awt]
#[tokio::test]
async fn second_challenge_within_cooldown_rejected(
    #[future] db: (AlwaysCloneableConnection, ContainerAsync<Postgres>),
) {
    let (conn, _c) = db;
    insert_user(&conn, 1, 1000).await;
    insert_user(&conn, 2, 1000).await;

    let registry = build_registry(conn.clone());
    let mg = registry.minigame_service();

    mg.pvp
        .challenge(UserId::from(1_i64), UserId::from(2_i64))
        .await
        .unwrap();
    let err = mg
        .pvp
        .challenge(UserId::from(1_i64), UserId::from(2_i64))
        .await
        .unwrap_err();
    assert!(matches!(err, PvpServiceError::OnCooldown));
}
