//! Economy / boonbuck-transfer tests. Uses the shared `ScenarioEnv` harness
//! (see `common.rs`) instead of a per-file registry/db/mock.

mod common;
use common::*;

use caliborn::{
    entities,
    repositories::{
        BaseRepository,
        users::{TransferError, UserRepositoryExt},
    },
    services::{UserId, economy::EconomyServiceError},
};
use rstest::rstest;

#[rstest]
#[awt]
#[tokio::test]
async fn transfer_succeeds_and_returns_balances(#[future] scenario: ScenarioEnv) {
    let env = scenario;
    env.insert_user(1, 100, 0).await;
    env.insert_user(2, 50, 0).await;

    let repo: BaseRepository<entities::users::Entity> = BaseRepository::new(&env.conn);
    let (sender, recipient) = repo.transfer_boonbucks(1, 2, 30).await.unwrap();

    assert_eq!(sender, 70);
    assert_eq!(recipient, 80);
    assert_eq!(env.balance(1).await, 70);
    assert_eq!(env.balance(2).await, 80);
}

#[rstest]
#[awt]
#[tokio::test]
async fn transfer_rejects_insufficient_funds(#[future] scenario: ScenarioEnv) {
    let env = scenario;
    env.insert_user(1, 10, 0).await;
    env.insert_user(2, 0, 0).await;

    let repo: BaseRepository<entities::users::Entity> = BaseRepository::new(&env.conn);
    let err = repo.transfer_boonbucks(1, 2, 50).await.unwrap_err();

    assert!(matches!(err, TransferError::InsufficientFunds));
    assert_eq!(env.balance(1).await, 10);
    assert_eq!(env.balance(2).await, 0);
}

#[rstest]
#[awt]
#[tokio::test]
async fn transfer_rejects_missing_sender(#[future] scenario: ScenarioEnv) {
    let env = scenario;
    env.insert_user(2, 0, 0).await;

    let repo: BaseRepository<entities::users::Entity> = BaseRepository::new(&env.conn);
    let err = repo.transfer_boonbucks(999, 2, 10).await.unwrap_err();

    assert!(matches!(err, TransferError::SenderNotFound));
}

#[rstest]
#[awt]
#[tokio::test]
async fn transfer_rejects_missing_recipient(#[future] scenario: ScenarioEnv) {
    let env = scenario;
    env.insert_user(1, 100, 0).await;

    let repo: BaseRepository<entities::users::Entity> = BaseRepository::new(&env.conn);
    let err = repo.transfer_boonbucks(1, 999, 10).await.unwrap_err();

    assert!(matches!(err, TransferError::RecipientNotFound));
    // Sender must be rolled back to original 100 because the second UPDATE failed.
    assert_eq!(env.balance(1).await, 100);
}

#[rstest]
#[awt]
#[tokio::test]
async fn pay_rejects_self_transfer(#[future] scenario: ScenarioEnv) {
    let env = scenario;
    env.insert_user(1, 100, 0).await;

    let economy = env.registry.economy_service();
    let err = economy
        .pay(UserId::from(1_i64), UserId::from(1_i64), 10)
        .await
        .unwrap_err();

    assert!(matches!(err, EconomyServiceError::SelfTransfer));
    assert_eq!(env.balance(1).await, 100);
}

#[rstest]
#[awt]
#[tokio::test]
async fn pay_rejects_invalid_amount(#[future] scenario: ScenarioEnv) {
    let env = scenario;
    env.insert_user(1, 100, 0).await;
    env.insert_user(2, 0, 0).await;

    let economy = env.registry.economy_service();
    let err = economy
        .pay(UserId::from(1_i64), UserId::from(2_i64), 0)
        .await
        .unwrap_err();

    assert!(matches!(err, EconomyServiceError::InvalidAmount));
    assert_eq!(env.balance(1).await, 100);
    assert_eq!(env.balance(2).await, 0);
}
