//! Kubernetes-style health probes.
//!
//! * `GET /health/livez` - is the process alive? Answers from the router alone
//!   and touches no dependency, so an orchestrator never restarts Caliborn just
//!   because Postgres blipped.
//! * `GET /health/readyz` - should this instance receive traffic? Pings the
//!   Postgres pool and answers `503` if that fails.
//!
//! Both are unauthenticated: probes run before any credential exists, and
//! neither leaks more than "the database is reachable".
//!
//! Liquidsoap is deliberately *not* probed. Its client sits behind a mutex
//! shared with request handlers, so a probe could queue behind an in-flight
//! command, and the vast majority of the API keeps working while the stream
//! host is down.

use axum::{Router, extract::State, routing::get};

use crate::{
    AppState, ServiceRegistry,
    dtos::health::{CheckStatus, LivenessDto, ReadinessChecks, ReadinessDto},
};

/// Liveness probe.
///
/// Always answers `200` while the process can serve requests.
#[utoipa::path(
    get,
    path = "/health/livez",
    responses(
        (status = 200, description = "The process is alive", body = LivenessDto)
    ),
    tags = ["Health"]
)]
pub async fn livez() -> LivenessDto {
    LivenessDto {
        status: CheckStatus::Ok,
    }
}

/// Readiness probe.
///
/// Pings the database pool. Answers `200` when every check passes and `503`
/// otherwise, so a load balancer can drain this instance.
#[utoipa::path(
    get,
    path = "/health/readyz",
    responses(
        (status = 200, description = "All dependency checks passed", body = ReadinessDto),
        (status = 503, description = "At least one dependency check failed", body = ReadinessDto)
    ),
    tags = ["Health"]
)]
pub async fn readyz(State(registry): State<ServiceRegistry>) -> ReadinessDto {
    let database = match registry.db_handle().ping().await {
        Ok(()) => CheckStatus::Ok,
        Err(e) => {
            tracing::warn!(error = ?e, "readiness probe: database ping failed");
            CheckStatus::Error
        }
    };

    ReadinessDto {
        status: database,
        checks: ReadinessChecks { database },
    }
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/livez", get(livez))
        .route("/readyz", get(readyz))
}
