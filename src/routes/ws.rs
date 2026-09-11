//! WebSocket endpoint that fans out [`crate::realtime::Event`]s to every
//! authenticated subscriber.
//!
//! Auth: pass a Caliborn JWT as `?token=<jwt>`. We accept JWT only here (no
//! API keys) because browsers cannot set custom headers on the upgrade
//! request and Calliope's session is JWT-shaped already.

use axum::{
    Router,
    extract::{
        Query, State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    response::IntoResponse,
    routing::get,
};
use reqwest::StatusCode;
use serde::Deserialize;
use tokio::{select, sync::broadcast::error::RecvError};
use tracing::{debug, warn};

use crate::{
    AppState,
    dtos::error::{ApiError, PublicError},
    realtime::Broadcaster,
};

/// Auth credential carried as a query parameter on the WS upgrade.
///
/// Exactly one of `token` (JWT) or `apikey` (Caliborn API key, `ak_…`) must
/// be set. Browsers cannot set custom headers on the upgrade request, hence
/// query-param auth.
#[derive(Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct WsQuery {
    pub token: Option<String>,
    pub apikey: Option<String>,
}

fn unauthorized(message: &'static str) -> ApiError {
    ApiError::Public(PublicError::new(
        "invalid-auth",
        message,
        StatusCode::UNAUTHORIZED,
    ))
}

/// Subscribe to the realtime event stream.
///
/// This is a WebSocket endpoint. OpenAPI cannot describe a WebSocket session —
/// 3.1 has no concept of one and 3.2 only added Server-Sent Events — so what is
/// documented here is the **handshake**: the `GET` that upgrades, its
/// query-param auth, and the `101` response. Once upgraded, the server pushes
/// JSON-encoded [`crate::realtime::Event`] values (`now_playing`,
/// `queue_updated`), discriminated by their `type` field, and never reads from
/// the client except to answer pings.
#[utoipa::path(
    get,
    path = "/ws",
    params(WsQuery),
    responses(
        (status = 101, description = "Switching Protocols — the WebSocket session is open and will stream `realtime::Event` values as JSON text frames"),
        (status = 400, description = "Not a valid WebSocket upgrade request", body = crate::dtos::error::ErrorResponse),
        (status = 401, description = "Missing, duplicated, or invalid `token`/`apikey` query parameter", body = crate::dtos::error::ErrorResponse),
    )
)]
pub async fn ws(
    Query(query): Query<WsQuery>,
    State(state): State<AppState>,
    upgrade: WebSocketUpgrade,
) -> Result<impl IntoResponse, ApiError> {
    match (query.token, query.apikey) {
        (Some(_), Some(_)) => {
            return Err(unauthorized(
                "Pass exactly one of `token` or `apikey`, not both.",
            ));
        }
        (None, None) => {
            return Err(unauthorized("Missing `token` or `apikey` query parameter."));
        }
        (Some(token), None) => {
            let claims = state.service_registry.auth_service().verify_token(&token)?;
            if !claims.is_valid_user() {
                return Err(unauthorized("Invalid auth token."));
            }
        }
        (None, Some(apikey)) => {
            // Re-uses the same check the bearer middleware uses; returns the
            // resolved user id, but we don't need it here — broadcasts are
            // public radio state.
            state
                .service_registry
                .auth_service()
                .check_api_key(&apikey)
                .await?;
        }
    }

    let broadcaster = state.service_registry.broadcaster().clone();
    Ok(upgrade.on_upgrade(move |socket| handle_socket(socket, broadcaster)))
}

async fn handle_socket(mut socket: WebSocket, broadcaster: Broadcaster) {
    let mut rx = broadcaster.subscribe();
    loop {
        select! {
            evt = rx.recv() => {
                match evt {
                    Ok(event) => {
                        let json = match serde_json::to_string(&event) {
                            Ok(s) => s,
                            Err(e) => {
                                warn!("ws: failed to serialize event: {e}");
                                continue;
                            }
                        };
                        if socket.send(Message::Text(json.into())).await.is_err() {
                            debug!("ws: client gone, closing");
                            break;
                        }
                    }
                    Err(RecvError::Lagged(skipped)) => {
                        warn!("ws: subscriber lagged, dropped {skipped} events");
                    }
                    Err(RecvError::Closed) => break,
                }
            }
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => {
                        debug!("ws: client closed");
                        break;
                    }
                    Some(Err(e)) => {
                        debug!("ws: recv error: {e}");
                        break;
                    }
                    Some(Ok(_)) => {
                        // Ignore inbound text/binary/ping/pong; axum handles
                        // ping/pong automatically.
                    }
                }
            }
        }
    }
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/ws", get(ws))
}
