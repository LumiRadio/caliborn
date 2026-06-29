//! Dice minigame tests. Uses the shared `ScenarioEnv` harness (see `common.rs`).

mod common;
use common::*;

use caliborn::{
    entities,
    services::{UserId, minigames::dice::DiceServiceError},
};
use rstest::rstest;
use sea_orm::{ActiveValue, EntityTrait, PaginatorTrait};

#[rstest]
#[awt]
#[tokio::test]
async fn roll_succeeds_and_records_history(#[future] scenario: ScenarioEnv) {
    let env = scenario;
    env.insert_user(1, 100, 0).await;
    let mg = env.registry.minigame_service();

    let result = mg.dice.roll(UserId::from(1_i64)).await.unwrap();

    assert_eq!(result.bet, 5);
    let net = result.payout - result.bet;
    assert_eq!(env.balance(1).await, 100 + net);
    assert_eq!(result.new_balance, 100 + net);
    assert_eq!(result.dice.len() as u8, result.mode);
    assert_eq!(result.won, result.payout > 0);

    let history_count = entities::minigame_history::Entity::find()
        .count(&*env.conn)
        .await
        .unwrap();
    assert_eq!(history_count, 1);
}

#[rstest]
#[awt]
#[tokio::test]
async fn roll_rejects_insufficient_funds(#[future] scenario: ScenarioEnv) {
    let env = scenario;
    env.insert_user(1, 4, 0).await;
    let err = env
        .registry
        .minigame_service()
        .dice
        .roll(UserId::from(1_i64))
        .await
        .unwrap_err();
    assert!(matches!(err, DiceServiceError::InsufficientFunds));
    assert_eq!(env.balance(1).await, 4);
}

#[rstest]
#[awt]
#[tokio::test]
async fn second_roll_within_cooldown_rejected(#[future] scenario: ScenarioEnv) {
    let env = scenario;
    env.insert_user(1, 1000, 0).await;
    let mg = env.registry.minigame_service();

    mg.dice.roll(UserId::from(1_i64)).await.unwrap();
    let err = mg.dice.roll(UserId::from(1_i64)).await.unwrap_err();
    assert!(matches!(err, DiceServiceError::OnCooldown));
}

#[rstest]
#[awt]
#[tokio::test]
async fn radio_state_target_unchanged_when_no_secret_match(#[future] scenario: ScenarioEnv) {
    let env = scenario;
    env.insert_user(1, 100, 0).await;

    // Force target to a value the player effectively cannot match by RNG (3-dice
    // mode means player_roll is 3 digits 1..6 each, e.g. 111..=666).
    // 999 is unreachable: 9 isn't a valid digit, so `player_roll == 999` is impossible.
    entities::radio_state::Entity::update(entities::radio_state::ActiveModel {
        id: ActiveValue::unchanged(1),
        dice_roll_target: ActiveValue::set(999),
        ..Default::default()
    })
    .exec(&*env.conn)
    .await
    .unwrap();

    let result = env
        .registry
        .minigame_service()
        .dice
        .roll(UserId::from(1_i64))
        .await
        .unwrap();
    assert!(!result.secret_match);
    assert_eq!(result.server_roll_before, 999);
    assert_eq!(result.server_roll_after, 999);

    let radio = entities::radio_state::Entity::find_by_id(1_i16)
        .one(&*env.conn)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(radio.dice_roll_target, 999);
}
