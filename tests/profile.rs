//! User-profile aggregation tests. Uses the shared `ScenarioEnv` harness
//! (see `common.rs`) instead of a per-file registry/db/mock.

mod common;
use common::*;

use caliborn::{entities, services::UserId};
use rstest::rstest;
use sea_orm::{ActiveValue, EntityTrait};

#[rstest]
#[awt]
#[tokio::test]
async fn profile_aggregates_balances_and_rank(#[future] scenario: ScenarioEnv) {
    let env = scenario;
    // Three users with different listening times and balances.
    env.insert_user(1, 100, 7200).await; // 2 hours
    env.insert_user(2, 500, 18000).await; // 5 hours, top hours
    env.insert_user(3, 1000, 3600).await; // 1 hour, top boonbucks

    let p1 = env
        .registry
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

    let p2 = env
        .registry
        .user_service()
        .get_profile(UserId::from(2_i64))
        .await
        .unwrap();
    assert_eq!(p2.position_by_hours, 1);

    let p3 = env
        .registry
        .user_service()
        .get_profile(UserId::from(3_i64))
        .await
        .unwrap();
    assert_eq!(p3.position_by_boonbucks, 1);
}

#[rstest]
#[awt]
#[tokio::test]
async fn profile_includes_can_count(#[future] scenario: ScenarioEnv) {
    let env = scenario;
    env.insert_user(1, 0, 0).await;

    // Insert 4 cans, only 3 legit.
    for legit in [true, true, true, false] {
        entities::cans::Entity::insert(entities::cans::ActiveModel {
            added_by: ActiveValue::set(1),
            legit: ActiveValue::set(legit),
            ..Default::default()
        })
        .exec(&*env.conn)
        .await
        .unwrap();
    }

    let p = env
        .registry
        .user_service()
        .get_profile(UserId::from(1_i64))
        .await
        .unwrap();
    assert_eq!(p.cans_added, 3);
}

#[rstest]
#[awt]
#[tokio::test]
async fn profile_auto_creates_missing_user(#[future] scenario: ScenarioEnv) {
    let env = scenario;
    let p = env
        .registry
        .user_service()
        .get_profile(UserId::from(42_i64))
        .await
        .unwrap();
    assert_eq!(p.id, 42);
    assert_eq!(p.boonbucks, 0);
    assert_eq!(p.cans_added, 0);
}
