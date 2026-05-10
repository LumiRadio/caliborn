//! Tests for the linked-roles persistence layer.
//!
//! The Discord HTTP calls (one-time `register_metadata` and per-user
//! `push_for_user`) hit `https://discord.com` directly and are not unit-
//! tested here — they would need an HTTPS mock server. We verify:
//!  - the `default_schema()` shape Discord expects
//!  - the `UserMetadata` JSON shape Discord expects
//!  - the `discord_role_connections` snapshot row roundtrips cleanly via
//!    sea-orm

use caliborn::{
    entities,
    repositories::AlwaysCloneableConnection,
    services::discord_linked_roles::{UserMetadata, default_schema},
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
async fn snapshot_roundtrips(#[future] db: (AlwaysCloneableConnection, ContainerAsync<Postgres>)) {
    let (conn, _c) = db;
    insert_user(&conn, 1).await;

    entities::discord_role_connections::Entity::insert(
        entities::discord_role_connections::ActiveModel {
            user_id: ActiveValue::set(1),
            last_pushed_at: ActiveValue::set(Some(chrono::Utc::now().naive_utc())),
            listening_hours_snapshot: ActiveValue::set(Some(10)),
            can_count_snapshot: ActiveValue::set(Some(2)),
            boonbucks_snapshot: ActiveValue::set(Some(500)),
        },
    )
    .exec(&*conn)
    .await
    .unwrap();

    let row = entities::discord_role_connections::Entity::find_by_id(1_i64)
        .one(&*conn)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.listening_hours_snapshot, Some(10));
    assert_eq!(row.can_count_snapshot, Some(2));
    assert_eq!(row.boonbucks_snapshot, Some(500));
}

#[test]
fn default_schema_matches_discord_expectations() {
    let s = default_schema();
    assert_eq!(s.len(), 3);
    let json = serde_json::to_string(&s).unwrap();
    assert!(json.contains("listening_hours"));
    assert!(json.contains("can_count"));
    assert!(json.contains("boonbucks"));
    // Discord requires the metadata `type` field; integer-gte = 2.
    assert!(json.contains("\"type\":2"));
}

#[test]
fn user_metadata_serializes_with_expected_keys() {
    let m = UserMetadata {
        listening_hours: 42,
        can_count: 5,
        boonbucks: 1234,
    };
    let json = serde_json::to_string(&m).unwrap();
    assert!(json.contains("\"listening_hours\":42"));
    assert!(json.contains("\"can_count\":5"));
    assert!(json.contains("\"boonbucks\":1234"));
}
