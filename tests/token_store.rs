//! Tests for `TokenStore` persistence. Refresh path requires a live Discord
//! token endpoint and is not exercised here. Uses the shared `ScenarioEnv`
//! harness (see `common.rs`); the store is read off `env.registry`.

mod common;
use common::*;

use rstest::rstest;

#[rstest]
#[awt]
#[tokio::test]
async fn store_and_fetch_roundtrips(#[future] scenario: ScenarioEnv) {
    let env = scenario;
    env.insert_user(1, 0, 0).await;

    let store = env.registry.token_store();
    let expires = chrono::Utc::now().naive_utc() + chrono::Duration::hours(1);
    store
        .store(1, "access-1", "refresh-1", expires, "identify connections")
        .await
        .unwrap();

    let got = store.fetch(1).await.unwrap();
    assert_eq!(got.access_token, "access-1");
    assert_eq!(got.refresh_token, "refresh-1");
    assert_eq!(got.scopes, "identify connections");
}

#[rstest]
#[awt]
#[tokio::test]
async fn store_overwrites_existing_row(#[future] scenario: ScenarioEnv) {
    let env = scenario;
    env.insert_user(1, 0, 0).await;

    let store = env.registry.token_store();
    let expires = chrono::Utc::now().naive_utc() + chrono::Duration::hours(1);
    store.store(1, "a1", "r1", expires, "scope1").await.unwrap();
    store.store(1, "a2", "r2", expires, "scope2").await.unwrap();

    let got = store.fetch(1).await.unwrap();
    assert_eq!(got.access_token, "a2");
    assert_eq!(got.refresh_token, "r2");
    assert_eq!(got.scopes, "scope2");
}

#[rstest]
#[awt]
#[tokio::test]
async fn fetch_returns_not_found_when_absent(#[future] scenario: ScenarioEnv) {
    use caliborn::services::discord_oauth_tokens::TokenStoreError;
    let env = scenario;

    let store = env.registry.token_store();
    let err = store.fetch(999).await.unwrap_err();
    assert!(matches!(err, TokenStoreError::NotFound(999)));
}

#[rstest]
#[awt]
#[tokio::test]
async fn valid_access_token_returns_stored_when_unexpired(#[future] scenario: ScenarioEnv) {
    let env = scenario;
    env.insert_user(1, 0, 0).await;

    let store = env.registry.token_store();
    let expires = chrono::Utc::now().naive_utc() + chrono::Duration::hours(1);
    store
        .store(1, "still-valid", "rt", expires, "identify")
        .await
        .unwrap();

    let token = store.valid_access_token(1).await.unwrap();
    assert_eq!(token, "still-valid");
}
