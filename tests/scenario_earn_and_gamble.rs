//! Cross-service scenario: a listener earns boonbucks from activity, then
//! gambles them. Spans UserService, CooldownService, MinigameService (slots),
//! the users + minigame_history tables, and the HTTP auth/route stack.

mod common;
use common::*;

use axum::http::{Method, StatusCode};
use caliborn::entities;
use rstest::rstest;
use sea_orm::EntityTrait;

/// Service-layer journey: activity credit -> top up -> slots debit/credit +
/// history -> cooldown blocks the immediate re-spin.
#[rstest]
#[awt]
#[tokio::test]
async fn activity_credits_then_slots_debits_then_cooldown_blocks(
    #[future] scenario: ScenarioEnv,
) {
    let env = scenario;
    let uid: i64 = 1001;
    env.insert_user(uid, 0, 0).await;

    // UserService.update_user_activity consults CooldownService, then credits
    // +3 boonbucks for a fresh (not-on-cooldown) user.
    env.registry
        .user_service()
        .update_user_activity(uid.into())
        .await
        .unwrap();
    assert_eq!(env.balance(uid).await, 3);

    // Top up to a known balance so the wager math is deterministic.
    env.registry
        .user_service()
        .update_user_boonbucks(uid.into(), 100)
        .await
        .unwrap();
    assert_eq!(env.balance(uid).await, 100);

    // MinigameService.slots.spin -> permission check (UserService) + cooldown
    // gate (CooldownService) + atomic balance update + minigame_history insert.
    let result = env
        .registry
        .minigame_service()
        .slots
        .spin(uid.into(), 5)
        .await
        .unwrap();
    assert_eq!(result.bet, 5);
    // new_balance = start - bet + payout, regardless of win/loss.
    assert_eq!(result.new_balance, 100 - 5 + result.payout);
    assert_eq!(env.balance(uid).await, result.new_balance);

    // History row was written by the spin.
    let history = entities::minigame_history::Entity::find()
        .all(&*env.conn)
        .await
        .unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].game, "slots");
    assert_eq!(history[0].wager, 5);
    assert_eq!(history[0].payout, result.payout);

    // Cross-service cooldown: spin sets SlotCooldown, so an immediate second
    // spin is rejected by CooldownService.
    let err = env
        .registry
        .minigame_service()
        .slots
        .spin(uid.into(), 5)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        caliborn::services::minigames::slots::SlotsServiceError::OnCooldown
    ));
}

/// HTTP journey over the real router: authenticated spin succeeds (200), the
/// returned balance matches the DB, and the immediate re-spin is rate-limited
/// (429) by the same cooldown path.
#[rstest]
#[awt]
#[tokio::test]
async fn slots_http_spins_then_429_on_cooldown(#[future] scenario: ScenarioEnv) {
    let env = scenario;
    let uid: i64 = 1002;
    env.insert_user(uid, 100, 0).await;
    let jwt = env.mint_jwt(uid);

    let (status, body) = env
        .request(
            Method::POST,
            "/minigames/slots/spin",
            Some(&jwt),
            Some(serde_json::json!({ "bet": 5 })),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let new_balance = body["new_balance"].as_i64().expect("new_balance in body");
    assert_eq!(env.balance(uid).await as i64, new_balance);

    // Second spin immediately -> SlotsServiceError::OnCooldown -> 429.
    let (status2, _) = env
        .request(
            Method::POST,
            "/minigames/slots/spin",
            Some(&jwt),
            Some(serde_json::json!({ "bet": 5 })),
        )
        .await;
    assert_eq!(status2, StatusCode::TOO_MANY_REQUESTS);
}

/// HTTP without a token is rejected before reaching any service.
#[rstest]
#[awt]
#[tokio::test]
async fn slots_http_requires_auth(#[future] scenario: ScenarioEnv) {
    let env = scenario;
    let (status, _) = env
        .request(
            Method::POST,
            "/minigames/slots/spin",
            None,
            Some(serde_json::json!({ "bet": 5 })),
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}
