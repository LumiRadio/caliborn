//! Cross-service scenario: a user requests a song. Spans UserService (activity
//! update), SongService, CooldownService (gate checks), the Liquidsoap client
//! (queue push), the song_requests table, and the HTTP auth/route stack.

mod common;
use common::*;

use axum::http::{Method, StatusCode};
use caliborn::entities;
use sea_orm::EntityTrait;

/// Service-layer: requesting an existing song pushes it to Liquidsoap and
/// records the request. The mock asserts the exact `srq.push <path>` command.
#[tokio::test]
async fn request_song_pushes_to_liquidsoap_and_records_request() {
    let mut ls = MockLiquidsoapClient::new();
    ls.expect_command_with_reconnect()
        .withf(|cmd| cmd == "srq.push /scenario/song.flac")
        .times(1)
        .returning(|_| Ok("Done.\nEND".to_string()));

    let env = scenario_with_liquidsoap(ls).await;
    let uid: i64 = 2001;
    env.insert_user(uid, 0, 0).await;
    env.insert_song("/scenario/song.flac", "scenariohash", 180.0)
        .await;

    let result = env
        .registry
        .song_service()
        .request_song(uid.into(), "scenariohash")
        .await
        .unwrap();
    assert_eq!(result.song.id, "scenariohash");

    // Cross-service: SongService inserted a song_requests row for this user.
    let reqs = entities::song_requests::Entity::find()
        .all(&*env.conn)
        .await
        .unwrap();
    assert_eq!(reqs.len(), 1);
    assert_eq!(reqs[0].song_id, "scenariohash");
    assert_eq!(reqs[0].user_id, uid);

    // The Liquidsoap mock's `.times(1)` expectation is verified on drop.
}

/// Requesting a song that does not exist surfaces SongNotFound (no Liquidsoap
/// command is sent — the mock has no expectations and would panic if called).
#[tokio::test]
async fn request_unknown_song_is_rejected() {
    let env = scenario_with_liquidsoap(MockLiquidsoapClient::new()).await;
    let uid: i64 = 2003;
    env.insert_user(uid, 0, 0).await;

    let result = env
        .registry
        .song_service()
        .request_song(uid.into(), "does-not-exist")
        .await;
    assert!(matches!(
        result,
        Err(caliborn::services::songs::SongServiceError::SongNotFound(_))
    ));
}

/// HTTP journey over the real router: an authenticated user (the default `user`
/// role has `use_bot`) requests a seeded song; the route runs auth ->
/// permission -> activity -> song request, pushes to Liquidsoap, and persists.
#[tokio::test]
async fn request_song_http_pushes_and_persists() {
    let mut ls = MockLiquidsoapClient::new();
    // Seeded song "abcd1234" lives at "/song1.mp3" (tests/fixtures/songs.yaml).
    ls.expect_command_with_reconnect()
        .withf(|cmd| cmd == "srq.push /song1.mp3")
        .times(1)
        .returning(|_| Ok("Done.\nEND".to_string()));

    let env = scenario_with_liquidsoap(ls).await;
    let uid: i64 = 2002;
    env.insert_user(uid, 0, 0).await;
    let jwt = env.mint_jwt(uid);

    let (status, body) = env
        .request(
            Method::POST,
            "/songs/request?file_hash=abcd1234",
            Some(&jwt),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["id"], "abcd1234");

    let reqs = entities::song_requests::Entity::find()
        .all(&*env.conn)
        .await
        .unwrap();
    assert_eq!(reqs.len(), 1);
    assert_eq!(reqs[0].song_id, "abcd1234");
    assert_eq!(reqs[0].user_id, uid);
}

/// HTTP without a token is rejected before reaching any service.
#[tokio::test]
async fn request_song_http_requires_auth() {
    let env = scenario_with_liquidsoap(MockLiquidsoapClient::new()).await;
    let (status, _) = env
        .request(
            Method::POST,
            "/songs/request?file_hash=abcd1234",
            None,
            None,
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}
