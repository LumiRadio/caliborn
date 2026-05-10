use std::sync::Arc;

use caliborn::{
    DiscordOAuthClient, ServiceRegistry, build_oauth2_client, entities,
    liquidsoap::LiquidsoapClient,
    repositories::AlwaysCloneableConnection,
    services::{UserId, minigames::dice::DiceServiceError},
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
async fn roll_succeeds_and_records_history(
    #[future] db: (AlwaysCloneableConnection, ContainerAsync<Postgres>),
) {
    let (conn, _c) = db;
    insert_user(&conn, 1, 100).await;
    let registry = build_registry(conn.clone());
    let mg = registry.minigame_service();

    let result = mg.dice.roll(UserId::from(1_i64)).await.unwrap();

    assert_eq!(result.bet, 5);
    let net = result.payout - result.bet;
    assert_eq!(balance(&conn, 1).await, 100 + net);
    assert_eq!(result.new_balance, 100 + net);
    assert_eq!(result.dice.len() as u8, result.mode);
    assert_eq!(result.won, result.payout > 0);

    let history_count = entities::minigame_history::Entity::find()
        .count(&*conn)
        .await
        .unwrap();
    assert_eq!(history_count, 1);
}

#[rstest]
#[awt]
#[tokio::test]
async fn roll_rejects_insufficient_funds(
    #[future] db: (AlwaysCloneableConnection, ContainerAsync<Postgres>),
) {
    let (conn, _c) = db;
    insert_user(&conn, 1, 4).await;
    let registry = build_registry(conn.clone());
    let err = registry
        .minigame_service()
        .dice
        .roll(UserId::from(1_i64))
        .await
        .unwrap_err();
    assert!(matches!(err, DiceServiceError::InsufficientFunds));
    assert_eq!(balance(&conn, 1).await, 4);
}

#[rstest]
#[awt]
#[tokio::test]
async fn second_roll_within_cooldown_rejected(
    #[future] db: (AlwaysCloneableConnection, ContainerAsync<Postgres>),
) {
    let (conn, _c) = db;
    insert_user(&conn, 1, 1000).await;
    let registry = build_registry(conn.clone());
    let mg = registry.minigame_service();

    mg.dice.roll(UserId::from(1_i64)).await.unwrap();
    let err = mg.dice.roll(UserId::from(1_i64)).await.unwrap_err();
    assert!(matches!(err, DiceServiceError::OnCooldown));
}

#[rstest]
#[awt]
#[tokio::test]
async fn radio_state_target_unchanged_when_no_secret_match(
    #[future] db: (AlwaysCloneableConnection, ContainerAsync<Postgres>),
) {
    use sea_orm::{ActiveValue::Set, EntityTrait};

    let (conn, _c) = db;
    insert_user(&conn, 1, 100).await;

    // Force target to a value the player effectively cannot match by RNG (3-dice
    // mode means player_roll is 3 digits 1..6 each, e.g. 111..=666).
    // 999 is unreachable: 9 isn't a valid digit, so `player_roll == 999` is impossible.
    entities::radio_state::Entity::update(entities::radio_state::ActiveModel {
        id: ActiveValue::unchanged(1),
        dice_roll_target: Set(999),
        ..Default::default()
    })
    .exec(&*conn)
    .await
    .unwrap();

    let registry = build_registry(conn.clone());
    let result = registry
        .minigame_service()
        .dice
        .roll(UserId::from(1_i64))
        .await
        .unwrap();
    assert!(!result.secret_match);
    assert_eq!(result.server_roll_before, 999);
    assert_eq!(result.server_roll_after, 999);

    let radio = entities::radio_state::Entity::find_by_id(1_i16)
        .one(&*conn)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(radio.dice_roll_target, 999);
}
