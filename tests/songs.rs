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

async fn search(app: axum::Router, query_string: &str) -> (u16, String) {
    let req = Request::builder()
        .method(Method::GET)
        .uri(format!("/songs/search?{query_string}"))
        .body(axum::body::Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    let status = response.status().as_u16();
    let bytes = axum::body::to_bytes(response.into_body(), 65536)
        .await
        .unwrap();

    (status, String::from_utf8_lossy(&bytes).into_owned())
}

fn titles(body: &str) -> Vec<String> {
    use caliborn::dtos::{page::Page, songs::SongDto};

    let page: Page<SongDto> = serde_json::from_str(body).expect("a page of songs");
    page.items.into_iter().map(|song| song.title).collect()
}

/// Postgres indexes `AC/DC` as the single lexeme `'ac/dc'`. The Rust vectorizer
/// tokenized it into `ac` and `dc` and required both, so `dc:*` matched nothing
/// and the band could not be found by its own name.
#[rstest]
#[awt]
#[tokio::test]
async fn test_search_matches_a_name_postgres_indexes_as_one_lexeme(
    #[future] app: (axum::Router, ContainerAsync<Postgres>),
) {
    let (app, _container) = app;

    let (status, body) = search(app, "query=AC%2FDC").await;

    assert_eq!(status, 200, "{body}");
    assert_eq!(titles(&body), vec!["Back in Black".to_string()]);
}

#[rstest]
#[awt]
#[tokio::test]
async fn test_search_with_only_stopwords_returns_no_rows_rather_than_failing(
    #[future] app: (axum::Router, ContainerAsync<Postgres>),
) {
    let (app, _container) = app;

    let (status, body) = search(app, "query=the").await;

    assert_eq!(status, 200, "{body}");
    assert!(titles(&body).is_empty(), "expected no matches, got {body}");
}

#[rstest]
#[awt]
#[tokio::test]
async fn test_search_with_tsquery_metacharacters_does_not_fail(
    #[future] app: (axum::Router, ContainerAsync<Postgres>),
    #[values("a%20%26%20b", "%21%21%21", "%3A%2A", "%28unbalanced", "%27quote")] query: &str,
) {
    let (app, _container) = app;

    let (status, body) = search(app, &format!("query={query}")).await;

    assert_eq!(status, 200, "{body}");
}

#[rstest]
#[awt]
#[tokio::test]
async fn test_search_matches_a_partial_word_as_a_prefix(
    #[future] app: (axum::Router, ContainerAsync<Postgres>),
) {
    let (app, _container) = app;

    let (status, body) = search(app, "query=unre").await;

    assert_eq!(status, 200, "{body}");
    let mut found = titles(&body);
    found.sort();
    assert_eq!(
        found,
        vec![
            "A Totally Unrelated Song".to_string(),
            "Totally Unrelated Song Two".to_string()
        ]
    );
}

/// "The Great Testing Song" matches `testing` in both its title and its album;
/// "Totally Unrelated Song Two" only in its album. Rank must prefer the former.
#[rstest]
#[awt]
#[tokio::test]
async fn test_search_ordered_by_relevance_ranks_the_stronger_match_first(
    #[future] app: (axum::Router, ContainerAsync<Postgres>),
) {
    let (app, _container) = app;

    let (status, body) = search(app, "query=testing&order%5Bby%5D=relevance").await;

    assert_eq!(status, 200, "{body}");
    let found = titles(&body);
    assert_eq!(
        found.first().map(String::as_str),
        Some("The Great Testing Song"),
        "got {found:?}"
    );
}

#[rstest]
#[awt]
#[tokio::test]
async fn test_relevance_order_without_a_search_term_falls_back_to_title_order(
    #[future] app: (axum::Router, ContainerAsync<Postgres>),
) {
    let (app, _container) = app;

    let (status, body) = search(app, "order%5Bby%5D=relevance").await;

    assert_eq!(status, 200, "{body}");
    let found = titles(&body);
    let mut sorted = found.clone();
    sorted.sort();
    assert_eq!(found, sorted, "expected title order, got {found:?}");
}

#[rstest]
#[awt]
#[tokio::test]
async fn test_songs_fulltext_tsvector_has_a_gin_index(
    #[future] db: (sea_orm::DatabaseConnection, ContainerAsync<Postgres>),
) {
    use sea_orm::{ConnectionTrait, Statement};

    let (db, _container) = db;

    let index = db
        .query_one_raw(Statement::from_string(
            sea_orm::DatabaseBackend::Postgres,
            "select indexdef from pg_indexes \
             where tablename = 'songs_fulltext' and indexdef ilike '%using gin%'",
        ))
        .await
        .expect("index lookup succeeds");

    assert!(
        index.is_some(),
        "songs_fulltext.tsvector has no GIN index; every search is a sequential scan"
    );
}
