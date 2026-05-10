use caliborn::{
    entities,
    repositories::{
        AlwaysCloneableConnection, BaseRepository,
        cooldowns::{CooldownScope, CreateCooldownDto},
    },
    services::cooldowns::{CooldownService, GlobalCooldown, global::CanCooldown},
};
use migration::MigratorTrait;
use rstest::{fixture, rstest};
use sea_orm::DatabaseConnection;
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

async fn insert_global_row(
    conn: &AlwaysCloneableConnection,
    key: &str,
    expires_at: chrono::NaiveDateTime,
) {
    BaseRepository::<entities::cooldown::Entity>::new(conn)
        .add(CreateCooldownDto {
            scope: CooldownScope::Global.to_string(),
            user_id: None,
            key: key.to_string(),
            expires_at,
        })
        .await
        .expect("failed to insert seed cooldown row");
}

#[rstest]
#[awt]
#[tokio::test]
async fn get_global_returns_none_for_expired_row(
    #[future] db: (AlwaysCloneableConnection, ContainerAsync<Postgres>),
) {
    let (conn, _container) = db;
    let service = CooldownService::new(&conn);

    let past = chrono::Utc::now().naive_utc() - chrono::Duration::seconds(600);
    insert_global_row(&conn, "can", past).await;

    let cooldown = service
        .get_global_cooldown("can")
        .await
        .expect("get should not fail");
    assert!(
        cooldown.is_none(),
        "expected expired cooldown to read as None, got {cooldown:?}",
    );
}

#[rstest]
#[awt]
#[tokio::test]
async fn set_global_overwrites_expired_row(
    #[future] db: (AlwaysCloneableConnection, ContainerAsync<Postgres>),
) {
    let (conn, _container) = db;
    let service = CooldownService::new(&conn);

    let past = chrono::Utc::now().naive_utc() - chrono::Duration::seconds(600);
    insert_global_row(&conn, "can", past).await;

    // Pre-fix this errored with `GlobalCooldownAlreadyExists`.
    service
        .set_global_cooldown("can", chrono::Duration::seconds(35))
        .await
        .expect("set should overwrite an expired row");

    let cooldown = service
        .get_global_cooldown("can")
        .await
        .expect("get should not fail")
        .expect("active cooldown should be present after set");
    assert!(
        cooldown > chrono::Utc::now().naive_utc(),
        "fresh cooldown should expire in the future, got {cooldown}",
    );
}

#[rstest]
#[awt]
#[tokio::test]
async fn cancooldown_full_cycle(
    #[future] db: (AlwaysCloneableConnection, ContainerAsync<Postgres>),
) {
    let (conn, _container) = db;
    let service = CooldownService::new(&conn);

    // Stale row in DB (e.g. left over from a previous run).
    let past = chrono::Utc::now().naive_utc() - chrono::Duration::seconds(900);
    insert_global_row(&conn, &CanCooldown.to_string(), past).await;

    assert!(
        !CanCooldown.on_cooldown(&service).await.unwrap(),
        "expired stale row must not be considered an active cooldown",
    );

    CanCooldown
        .set(&service)
        .await
        .expect("setting after expiry should succeed");

    assert!(
        CanCooldown.on_cooldown(&service).await.unwrap(),
        "freshly set cooldown should be active",
    );
    let expires_at = CanCooldown
        .get(&service)
        .await
        .unwrap()
        .expect("active cooldown should read back");
    let remaining = expires_at
        .signed_duration_since(chrono::Utc::now().naive_utc())
        .num_seconds();
    assert!(
        remaining > 0 && remaining <= 35,
        "remaining should be within the 35s CanCooldown window, got {remaining}s",
    );
}
