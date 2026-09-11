//! Health-probe response bodies (`/health/livez`, `/health/readyz`).

use axum::response::IntoResponse;
use reqwest::StatusCode;
use serde::Serialize;
use utoipa::ToSchema;

use crate::dtos::json_with_status;

/// Outcome of a single dependency check.
#[derive(Serialize, ToSchema, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    /// The dependency answered.
    Ok,
    /// The dependency is unreachable or errored.
    Error,
}

/// Liveness response. The process is running and the router is serving.
#[derive(Serialize, ToSchema)]
#[schema(examples(json!({"status": "ok"})))]
pub struct LivenessDto {
    /// Always `ok` — a non-200 here means the process is gone, not degraded.
    pub status: CheckStatus,
}

impl IntoResponse for LivenessDto {
    fn into_response(self) -> axum::response::Response {
        json_with_status(self, StatusCode::OK).into_response()
    }
}

/// Readiness response. `status` is `ok` only if every dependency check passed.
#[derive(Serialize, ToSchema)]
#[schema(examples(json!({"status": "ok", "checks": {"database": "ok"}})))]
pub struct ReadinessDto {
    /// Aggregate of `checks` — `ok` (200) or `error` (503).
    pub status: CheckStatus,
    /// Per-dependency results.
    pub checks: ReadinessChecks,
}

/// The individual dependency checks `/health/readyz` performs.
#[derive(Serialize, ToSchema)]
pub struct ReadinessChecks {
    /// Result of a `SELECT 1` against the Postgres pool.
    pub database: CheckStatus,
}

impl IntoResponse for ReadinessDto {
    fn into_response(self) -> axum::response::Response {
        let status = match self.status {
            CheckStatus::Ok => StatusCode::OK,
            CheckStatus::Error => StatusCode::SERVICE_UNAVAILABLE,
        };
        json_with_status(self, status).into_response()
    }
}
