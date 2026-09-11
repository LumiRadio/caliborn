//! The generated OpenAPI document is Calliope's contract. Every route the
//! router serves must appear in it, with the same path the router mounts it
//! under (handler `path = ` strings include the `nest()` prefix).

use utoipa::OpenApi;

fn spec() -> serde_json::Value {
    serde_json::to_value(caliborn::openapi::ApiDoc::openapi()).unwrap()
}

fn assert_documented(spec: &serde_json::Value, path: &str, method: &str) {
    let item = spec
        .pointer(&format!(
            "/paths/{}",
            path.replace('~', "~0").replace('/', "~1")
        ))
        .unwrap_or_else(|| panic!("`{path}` missing from the OpenAPI document"));
    assert!(
        item.get(method).is_some(),
        "`{method} {path}` missing from the OpenAPI document (have: {:?})",
        item.as_object().map(|o| o.keys().collect::<Vec<_>>())
    );
}

#[test]
fn song_routes_are_documented() {
    let spec = spec();
    for (path, method) in [
        ("/songs/request", "post"),
        ("/songs/queue", "get"),
        ("/songs/history", "get"),
        ("/songs/search", "get"),
        ("/songs/current", "get"),
        ("/songs/favourites", "get"),
        ("/songs/favourite", "post"),
        ("/songs/favourite", "delete"),
        ("/songs/favourite/current", "post"),
    ] {
        assert_documented(&spec, path, method);
    }
}

#[test]
fn admin_crud_routes_are_documented() {
    let spec = spec();
    for (path, method) in [
        ("/admin/crud/_meta", "get"),
        ("/admin/crud/{resource}", "get"),
        ("/admin/crud/{resource}", "post"),
        ("/admin/crud/{resource}/_schema", "get"),
        ("/admin/crud/{resource}/{id}", "get"),
        ("/admin/crud/{resource}/{id}", "put"),
        ("/admin/crud/{resource}/{id}", "delete"),
    ] {
        assert_documented(&spec, path, method);
    }
}

/// OpenAPI has no WebSocket concept (3.1 has none; 3.2 only added SSE), so the
/// best we can do is document the handshake `GET` and its `101` upgrade so the
/// endpoint and its auth query params are discoverable in Swagger.
#[test]
fn websocket_handshake_is_documented() {
    let spec = spec();
    assert_documented(&spec, "/ws", "get");
    assert!(
        spec.pointer("/paths/~1ws/get/responses/101").is_some(),
        "`/ws` should document the 101 Switching Protocols upgrade"
    );
}
