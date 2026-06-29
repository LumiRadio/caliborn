//! Tests for the linked-roles persistence layer. The Discord HTTP calls are
//! not exercised here (they would need an HTTPS mock). Uses the shared
//! `ScenarioEnv` harness (see `common.rs`) for the DB-backed snapshot test.

mod common;
use common::*;

use caliborn::{
    entities,
    services::discord_linked_roles::{UserMetadata, default_schema},
};
use rstest::rstest;
use sea_orm::{ActiveValue, EntityTrait};

#[rstest]
#[awt]
#[tokio::test]
async fn snapshot_roundtrips(#[future] scenario: ScenarioEnv) {
    let env = scenario;
    env.insert_user(1, 0, 0).await;

    entities::discord_role_connections::Entity::insert(
        entities::discord_role_connections::ActiveModel {
            user_id: ActiveValue::set(1),
            last_pushed_at: ActiveValue::set(Some(chrono::Utc::now().naive_utc())),
            listening_hours_snapshot: ActiveValue::set(Some(10)),
            can_count_snapshot: ActiveValue::set(Some(2)),
            boonbucks_snapshot: ActiveValue::set(Some(500)),
        },
    )
    .exec(&*env.conn)
    .await
    .unwrap();

    let row = entities::discord_role_connections::Entity::find_by_id(1_i64)
        .one(&*env.conn)
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
