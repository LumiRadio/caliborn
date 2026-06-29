//! Slots minigame tests. Uses the shared `ScenarioEnv` harness (see `common.rs`).

mod common;
use common::*;

use caliborn::{
    entities,
    services::{UserId, minigames::slots::SlotsServiceError},
};
use rstest::rstest;
use sea_orm::{EntityTrait, PaginatorTrait};

#[rstest]
#[awt]
#[tokio::test]
async fn spin_succeeds_and_records_history(#[future] scenario: ScenarioEnv) {
    let env = scenario;
    env.insert_user(1, 100, 0).await;
    let mg = env.registry.minigame_service();

    let result = mg.slots.spin(UserId::from(1_i64), 5).await.unwrap();

    assert_eq!(result.bet, 5);
    assert_eq!(result.payout, 5 * result.multiplier as i32);
    assert_eq!(result.won, result.multiplier > 0);
    let net = result.payout - result.bet;
    assert_eq!(env.balance(1).await, 100 + net);
    assert_eq!(result.new_balance, 100 + net);

    let history_count = entities::minigame_history::Entity::find()
        .count(&*env.conn)
        .await
        .unwrap();
    assert_eq!(history_count, 1);
}

#[rstest]
#[awt]
#[tokio::test]
async fn spin_rejects_bet_below_minimum(#[future] scenario: ScenarioEnv) {
    let env = scenario;
    env.insert_user(1, 100, 0).await;
    let err = env
        .registry
        .minigame_service()
        .slots
        .spin(UserId::from(1_i64), 0)
        .await
        .unwrap_err();
    assert!(matches!(err, SlotsServiceError::BetOutOfRange));
    assert_eq!(env.balance(1).await, 100);
}

#[rstest]
#[awt]
#[tokio::test]
async fn spin_rejects_bet_above_maximum(#[future] scenario: ScenarioEnv) {
    let env = scenario;
    env.insert_user(1, 100, 0).await;
    let err = env
        .registry
        .minigame_service()
        .slots
        .spin(UserId::from(1_i64), 11)
        .await
        .unwrap_err();
    assert!(matches!(err, SlotsServiceError::BetOutOfRange));
    assert_eq!(env.balance(1).await, 100);
}

#[rstest]
#[awt]
#[tokio::test]
async fn spin_rejects_insufficient_funds(#[future] scenario: ScenarioEnv) {
    let env = scenario;
    env.insert_user(1, 2, 0).await;
    let err = env
        .registry
        .minigame_service()
        .slots
        .spin(UserId::from(1_i64), 5)
        .await
        .unwrap_err();
    assert!(matches!(err, SlotsServiceError::InsufficientFunds));
    assert_eq!(env.balance(1).await, 2);
}

#[rstest]
#[awt]
#[tokio::test]
async fn second_spin_within_cooldown_rejected(#[future] scenario: ScenarioEnv) {
    let env = scenario;
    env.insert_user(1, 1000, 0).await;
    let mg = env.registry.minigame_service();

    mg.slots.spin(UserId::from(1_i64), 1).await.unwrap();
    let err = mg.slots.spin(UserId::from(1_i64), 1).await.unwrap_err();
    assert!(matches!(err, SlotsServiceError::OnCooldown));
}
