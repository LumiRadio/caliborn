use axum::extract::{Query, State};
use serde::Deserialize;
use utoipa::IntoParams;

use crate::{AppState, dtos::error::CalibornResult, services::auth::AuthenticatedUser};

#[derive(Deserialize, Debug, IntoParams)]
#[into_params(parameter_in = Query, rename_all = "snake_case")]
pub struct UpdateUsernameQuery {
    /// The new username to set for the user.
    username: String,
}

/// Updates a user's username.
///
/// Despite the OpenAPI documentation, this endpoint is authenticated
/// and requires a valid HMAC signature in the request headers.
#[utoipa::path(
    post,
    path = "/bot/update_username",
    params(
        UpdateUsernameQuery
    ),
    responses(
        (status = 200, description = "Username updated successfully"),
        (status = 500, description = "An internal server error occurred", body = crate::dtos::error::ErrorResponse, example = json!({"message": "Internal server error", "error": "Internal Server Error"}))
    ),
    tags = ["Bot"],
)]
pub async fn update_username(
    AuthenticatedUser(actor): AuthenticatedUser,
    State(state): State<AppState>,
    Query(params): Query<UpdateUsernameQuery>,
) -> CalibornResult<()> {
    let user_service = state.service_registry.user_service();
    user_service
        .set_username(actor.user_id(), params.username)
        .await?;

    Ok(())
}

pub fn routes(state: AppState) -> axum::Router<AppState> {
    axum::Router::new()
        .route("/update_username", axum::routing::post(update_username))
        .layer(axum::middleware::from_fn_with_state(
            state,
            crate::services::auth::authenticate_hmac,
        ))
}
