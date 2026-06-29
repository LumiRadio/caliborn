//! PvP minigame tests. Uses the shared `ScenarioEnv` harness (see `common.rs`).

mod common;
use common::*;

use caliborn::{
    entities,
    services::{UserId, minigames::pvp::PvpServiceError},
};
use rstest::rstest;
use sea_orm::{EntityTrait, PaginatorTrait};

#[rstest]
#[awt]
#[tokio::test]
async fn challenge_resolves_and_transfers_bet(#[future] scenario: ScenarioEnv) {
    let env = scenario;
    env.insert_user(1, 100, 0).await;
    env.insert_user(2, 100, 0).await;

    let result = env
        .registry
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
        assert_eq!(env.balance(1).await, 110);
        assert_eq!(env.balance(2).await, 90);
    } else {
        assert_eq!(result.challenger_balance, 90);
        assert_eq!(result.opponent_balance, 110);
        assert_eq!(env.balance(1).await, 90);
        assert_eq!(env.balance(2).await, 110);
    }

    let history_count = entities::minigame_history::Entity::find()
        .count(&*env.conn)
        .await
        .unwrap();
    assert_eq!(history_count, 1);
}

#[rstest]
#[awt]
#[tokio::test]
async fn challenge_rejects_self(#[future] scenario: ScenarioEnv) {
    let env = scenario;
    env.insert_user(1, 100, 0).await;

    let err = env
        .registry
        .minigame_service()
        .pvp
        .challenge(UserId::from(1_i64), UserId::from(1_i64))
        .await
        .unwrap_err();
    assert!(matches!(err, PvpServiceError::SelfChallenge));
    assert_eq!(env.balance(1).await, 100);
}

#[rstest]
#[awt]
#[tokio::test]
async fn challenge_rejects_challenger_short_on_funds(#[future] scenario: ScenarioEnv) {
    let env = scenario;
    env.insert_user(1, 5, 0).await;
    env.insert_user(2, 100, 0).await;

    let err = env
        .registry
        .minigame_service()
        .pvp
        .challenge(UserId::from(1_i64), UserId::from(2_i64))
        .await
        .unwrap_err();
    assert!(matches!(err, PvpServiceError::ChallengerInsufficientFunds));
    assert_eq!(env.balance(1).await, 5);
    assert_eq!(env.balance(2).await, 100);
}

#[rstest]
#[awt]
#[tokio::test]
async fn challenge_rejects_opponent_short_on_funds(#[future] scenario: ScenarioEnv) {
    let env = scenario;
    env.insert_user(1, 100, 0).await;
    env.insert_user(2, 5, 0).await;

    let err = env
        .registry
        .minigame_service()
        .pvp
        .challenge(UserId::from(1_i64), UserId::from(2_i64))
        .await
        .unwrap_err();
    assert!(matches!(err, PvpServiceError::OpponentInsufficientFunds));
    assert_eq!(env.balance(1).await, 100);
    assert_eq!(env.balance(2).await, 5);
}

#[rstest]
#[awt]
#[tokio::test]
async fn second_challenge_within_cooldown_rejected(#[future] scenario: ScenarioEnv) {
    let env = scenario;
    env.insert_user(1, 1000, 0).await;
    env.insert_user(2, 1000, 0).await;

    let mg = env.registry.minigame_service();
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
