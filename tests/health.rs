//! Health-probe tests. Both probes must answer without any credential, and
//! `readyz` must actually report on the database rather than always saying "ok".

mod common;
use common::*;

use axum::http::{Method, StatusCode};

#[tokio::test]
async fn livez_is_unauthenticated_and_ok() {
    let env = scenario().await;

    let (status, body) = env.request(Method::GET, "/health/livez", None, None).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ok");
}

#[tokio::test]
async fn readyz_reports_a_healthy_database() {
    let env = scenario().await;

    let (status, body) = env.request(Method::GET, "/health/readyz", None, None).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ok");
    assert_eq!(body["checks"]["database"], "ok");
}

#[tokio::test]
async fn readyz_is_unavailable_when_the_database_is_gone() {
    let env = scenario().await;

    // Kill the database out from under the running router; the pool's next
    // ping has nothing to talk to.
    env.stop_database().await;

    let (status, body) = env.request(Method::GET, "/health/readyz", None, None).await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["status"], "error");
    assert_eq!(body["checks"]["database"], "error");
}

#[test]
fn health_routes_are_documented() {
    use utoipa::OpenApi;

    let spec = serde_json::to_value(caliborn::openapi::ApiDoc::openapi()).unwrap();
    for path in ["~1health~1livez", "~1health~1readyz"] {
        let item = spec
            .pointer(&format!("/paths/{path}"))
            .unwrap_or_else(|| panic!("`{path}` missing from the OpenAPI document"));
        assert!(item.get("get").is_some(), "`get {path}` missing");
    }
}
