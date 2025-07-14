use axum::http::Request;
use reqwest::Method;
use rstest::rstest;

mod common;

use common::*;
use testcontainers::ContainerAsync;
use testcontainers_modules::postgres::Postgres;
use tower::ServiceExt;

#[rstest]
#[awt]
#[tokio::test]
async fn test_song_search(
    #[future] app: (axum::Router, ContainerAsync<Postgres>),
    #[values("test", "unrelated")] query: &str,
) {
    use caliborn::dtos::{page::Page, songs::SongDto};

    let (app, _container) = app;
    let req = Request::builder()
        .method(Method::GET)
        .uri(format!("/songs/search?query={}", query))
        .body(axum::body::Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    let status = response.status();
    let body = response.into_body();
    let bytes = axum::body::to_bytes(body, 4096).await.unwrap();
    let body = String::from_utf8_lossy(&bytes);
    assert_eq!(status, 200);
    let body: Page<SongDto> = serde_json::from_str(&body).unwrap();
    assert!(!body.items.is_empty());
}
