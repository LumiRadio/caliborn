//! Generic admin CRUD registry: resources reachable through
//! `/admin/crud/{resource}`.
//!
//! Every table with a single-column primary key is registrable. These tests
//! cover the resources added on top of the original nine, plus the `_meta`
//! discovery endpoint the frontend uses to enumerate them.

mod common;
use common::*;

use axum::http::{Method, StatusCode};
use shared_constants::permissions::PERM_USE_ADMIN_CRUD;

const ADMIN: i64 = 900_001;

/// Build an env with one permission-carrying admin user and return their JWT.
async fn admin_env() -> (ScenarioEnv, String) {
    let env = scenario().await;
    env.insert_user(ADMIN, 0, 0).await;
    env.grant_permission(ADMIN, PERM_USE_ADMIN_CRUD.name).await;
    let jwt = env.mint_jwt(ADMIN);
    (env, jwt)
}

#[tokio::test]
async fn meta_lists_every_single_pk_resource() {
    let (env, jwt) = admin_env().await;

    let (status, body) = env
        .request(Method::GET, "/admin/crud/_meta", Some(&jwt), None)
        .await;

    assert_eq!(status, StatusCode::OK);
    let names: Vec<String> = serde_json::from_value(body).unwrap();

    for expected in [
        "connected_youtube_accounts",
        "played_songs",
        "slcb_currency",
        "slcb_rank",
        "song_tags",
    ] {
        assert!(
            names.iter().any(|n| n == expected),
            "`{expected}` missing from /admin/crud/_meta: {names:?}"
        );
    }
}

#[tokio::test]
async fn slcb_rank_round_trips_through_crud() {
    let (env, jwt) = admin_env().await;

    let (status, created) = env
        .request(
            Method::POST,
            "/admin/crud/slcb_rank",
            Some(&jwt),
            Some(serde_json::json!({
                "rank_name": "Prospit Dreamer",
                "hour_requirement": 10,
                "channel_id": null,
            })),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "create failed: {created}");
    let id = created["id"].as_i64().expect("created row has an id");
    assert_eq!(created["rank_name"], "Prospit Dreamer");

    let (status, read) = env
        .request(
            Method::GET,
            &format!("/admin/crud/slcb_rank/{id}"),
            Some(&jwt),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(read["hour_requirement"], 10);

    let (status, edited) = env
        .request(
            Method::PUT,
            &format!("/admin/crud/slcb_rank/{id}"),
            Some(&jwt),
            Some(serde_json::json!({ "hour_requirement": 25 })),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "edit failed: {edited}");
    assert_eq!(edited["hour_requirement"], 25);
    assert_eq!(
        edited["rank_name"], "Prospit Dreamer",
        "partial update must not clobber untouched columns"
    );

    let (status, _) = env
        .request(
            Method::DELETE,
            &format!("/admin/crud/slcb_rank/{id}"),
            Some(&jwt),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    let (status, read) = env
        .request(
            Method::GET,
            &format!("/admin/crud/slcb_rank/{id}"),
            Some(&jwt),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(read.is_null(), "deleted row still readable: {read}");
}

#[tokio::test]
async fn song_tags_are_creatable_and_listable() {
    let (env, jwt) = admin_env().await;
    env.insert_song("/crud/tagged.flac", "taghash", 120.0).await;

    let (status, created) = env
        .request(
            Method::POST,
            "/admin/crud/song_tags",
            Some(&jwt),
            Some(serde_json::json!({
                "song_id": "taghash",
                "tag": "mood",
                "value": "melancholy",
            })),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "create failed: {created}");

    let (status, page) = env
        .request(Method::GET, "/admin/crud/song_tags", Some(&jwt), None)
        .await;
    assert_eq!(status, StatusCode::OK);
    let items = page["items"].as_array().expect("page has items: {page}");
    assert!(
        items
            .iter()
            .any(|i| i["tag"] == "mood" && i["value"] == "melancholy"),
        "created tag not in listing: {page}"
    );
}

#[tokio::test]
async fn connected_youtube_accounts_are_creatable() {
    let (env, jwt) = admin_env().await;
    env.insert_user(900_002, 0, 0).await;

    let (status, created) = env
        .request(
            Method::POST,
            "/admin/crud/connected_youtube_accounts",
            Some(&jwt),
            Some(serde_json::json!({
                "user_id": 900_002,
                "youtube_channel_id": "UCtest",
                "youtube_channel_name": "Test Channel",
            })),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "create failed: {created}");
    assert_eq!(created["youtube_channel_id"], "UCtest");
}

#[tokio::test]
async fn slcb_currency_is_editable() {
    let (env, jwt) = admin_env().await;

    let (status, created) = env
        .request(
            Method::POST,
            "/admin/crud/slcb_currency",
            Some(&jwt),
            Some(serde_json::json!({
                "username": "someviewer",
                "points": 100,
                "hours": 5,
                "user_id": null,
            })),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "create failed: {created}");
    let id = created["id"].as_i64().unwrap();

    let (status, edited) = env
        .request(
            Method::PUT,
            &format!("/admin/crud/slcb_currency/{id}"),
            Some(&jwt),
            Some(serde_json::json!({ "points": 250 })),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "edit failed: {edited}");
    assert_eq!(edited["points"], 250);
    assert_eq!(edited["hours"], 5);
}

#[tokio::test]
async fn played_songs_are_listable() {
    let (env, jwt) = admin_env().await;

    let (status, page) = env
        .request(Method::GET, "/admin/crud/played_songs", Some(&jwt), None)
        .await;

    assert_eq!(
        status,
        StatusCode::OK,
        "played_songs not registered: {page}"
    );
    assert!(page["items"].is_array(), "unexpected page shape: {page}");
}
