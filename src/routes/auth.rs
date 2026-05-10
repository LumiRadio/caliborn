use axum::{Router, extract::State, routing::get};

use crate::{
    AppState,
    dtos::{
        Query,
        auth::{DiscordLoginQuery, UserToken},
        error::{CalibornResult, ErrorResponse},
    },
    services::ServiceRegistry,
};

/// Logs in a user via Discord
///
/// Exchanges the authorization code received from Discord (via the standard
/// authorization-code flow) for a Caliborn JWT.
///
/// The frontend redirects the user to the
/// [Discord authorization URL](https://discord.com/developers/docs/topics/oauth2#authorization-code-flow);
/// Discord redirects back with `?code=...`; the frontend then calls this
/// endpoint with that code as a query parameter.
#[utoipa::path(
    get,
    path = "/auth/discord/login",
    params(DiscordLoginQuery),
    responses(
        (status = 200, description = "Discord login was successful", body = UserToken),
        (status = 401, description = "An authorization error occurred (e.g. invalid authorization code)", body = ErrorResponse, example = json!({"message": "Invalid authorization code", "error": "Unauthorized"})),
        (status = 500, description = "An internal server error occurred", body = ErrorResponse, example = json!({"message": "Internal server error", "error": "Internal Server Error"}))
    )
)]
#[axum::debug_handler]
pub async fn discord_login(
    State(registry): State<ServiceRegistry>,
    Query(params): Query<DiscordLoginQuery>,
) -> CalibornResult<UserToken> {
    let auth_service = registry.auth_service();
    let token = auth_service.login_user(&params.code).await?;

    Ok(token)
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/discord/login", get(discord_login))
}
