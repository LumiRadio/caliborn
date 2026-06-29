//! CooldownService tests. Uses the shared `ScenarioEnv` harness (see
//! `common.rs`); the cooldown service is read off `env.registry`.

mod common;
use common::*;

use caliborn::{
    entities,
    repositories::{
        BaseRepository,
        cooldowns::{CooldownScope, CreateCooldownDto},
    },
    services::cooldowns::{GlobalCooldown, global::CanCooldown},
};
use rstest::rstest;

async fn insert_global_row(env: &ScenarioEnv, key: &str, expires_at: chrono::NaiveDateTime) {
    BaseRepository::<entities::cooldown::Entity>::new(&env.conn)
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
async fn get_global_returns_none_for_expired_row(#[future] scenario: ScenarioEnv) {
    let env = scenario;
    let service = env.registry.cooldown_service();

    let past = chrono::Utc::now().naive_utc() - chrono::Duration::seconds(600);
    insert_global_row(&env, "can", past).await;

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
async fn set_global_overwrites_expired_row(#[future] scenario: ScenarioEnv) {
    let env = scenario;
    let service = env.registry.cooldown_service();

    let past = chrono::Utc::now().naive_utc() - chrono::Duration::seconds(600);
    insert_global_row(&env, "can", past).await;

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
async fn cancooldown_full_cycle(#[future] scenario: ScenarioEnv) {
    let env = scenario;
    let service = env.registry.cooldown_service();

    // Stale row in DB (e.g. left over from a previous run).
    let past = chrono::Utc::now().naive_utc() - chrono::Duration::seconds(900);
    insert_global_row(&env, &CanCooldown.to_string(), past).await;

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
