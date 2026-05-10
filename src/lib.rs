// #![warn(missing_docs)]

//! Caliborn is an API server for LumiRadio. It provides the backend for the Discord bot and web frontend, Calliope.

use std::sync::Arc;

use axum::Router;
use axum_macros::FromRef;
use hmac::Hmac;
use oauth2::{ClientId, ClientSecret, EndpointNotSet, EndpointSet, RedirectUrl, TokenUrl};
use sha2::Sha256;
use tokio::sync::Mutex;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use crate::{
    liquidsoap::LiquidsoapClient, openapi::ApiDoc, realtime::Broadcaster,
    repositories::AlwaysCloneableConnection,
};
pub use crate::{
    liquidsoap::{LiquidsoapClientImpl, LiquidsoapError},
    realtime::{Broadcaster as RealtimeBroadcaster, Event as RealtimeEvent},
    repositories::RepositoryError,
    services::ServiceRegistry,
};

mod openapi;

/// Data Transfer Objects (DTOs) used for communication between services.
pub mod dtos;
pub mod entities;
pub mod fixtures;
pub mod liquidsoap;
pub mod pg_extension;
/// Realtime fan-out (WebSocket events).
pub mod realtime;
/// Database repositories for accessing the database.
pub mod repositories;
/// API routes for the application.
pub mod routes;
pub mod sea_orm_utils;
/// Services for business logic.
pub mod services;
/// Vectorizer for full-text search.
pub mod vectorizer;

/// A type alias for a Discord OAuth client, which doesn't need authorization, but only the code exchange.
pub type DiscordOAuthClient = oauth2::basic::BasicClient<
    EndpointNotSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointSet,
>;

/// Caliborn's application state, consisting of:
///
/// - `service_registry`: A registry of services, which provides access to the services used by the application.
#[derive(Clone, FromRef)]
pub struct AppState {
    /// A registry of services, which provides access to the services used by the application.
    pub service_registry: ServiceRegistry,
}

/// Builds a Discord OAuth client.
///
/// # Arguments
///
/// * `client_id` - The client ID of the Discord application.
/// * `client_secret` - The client secret of the Discord application.
/// * `redirect_uri` - The redirect URI of the Discord application.
///
/// # Returns
///
/// A `Result` containing the `DiscordOAuthClient` or a `url::ParseError` if the redirect URI is invalid.
pub fn build_oauth2_client(
    client_id: &str,
    client_secret: &str,
    redirect_uri: &str,
) -> Result<DiscordOAuthClient, url::ParseError> {
    let client = oauth2::basic::BasicClient::new(ClientId::new(client_id.to_string()))
        .set_client_secret(ClientSecret::new(client_secret.to_string()))
        .set_token_uri(TokenUrl::new(
            "https://discord.com/api/v10/oauth2/token".to_string(),
        )?)
        .set_redirect_uri(RedirectUrl::new(redirect_uri.to_string())?);

    Ok(client)
}

/// Builds an axum router for the application.
///
/// # Arguments
///
/// * `jwt_secret` - The secret used for JWT authentication.
/// * `oauth_client` - The Discord OAuth client.
/// * `factory` - A repository factory, which implements the [`RepositoryFactory`] trait.
///
/// # Returns
///
/// An `axum::Router` containing the application routes.
pub fn make_app(
    jwt_secret: Hmac<Sha256>,
    hmac_secret: Hmac<Sha256>,
    oauth_client: DiscordOAuthClient,
    db: AlwaysCloneableConnection,
    liquidsoap_client: Arc<Mutex<dyn LiquidsoapClient>>,
    discord_application_id: String,
    linked_roles_platform_name: String,
) -> axum::Router {
    let broadcaster = Broadcaster::new();
    let app_state = AppState {
        service_registry: ServiceRegistry::new(
            db,
            jwt_secret,
            hmac_secret,
            oauth_client,
            liquidsoap_client,
            broadcaster,
            discord_application_id,
            linked_roles_platform_name,
        ),
    };

    let router = Router::new()
        .nest("/auth", routes::auth::routes())
        .nest("/user", routes::user::routes(app_state.clone()))
        .nest("/cans", routes::cans::routes(app_state.clone()))
        .nest("/bears", routes::bears::routes(app_state.clone()))
        .nest("/minigames", routes::minigames::routes(app_state.clone()))
        .nest("/songs", routes::songs::routes(app_state.clone()))
        .nest("/stream", routes::stream::routes(app_state.clone()))
        .nest("/playback", routes::playback::routes())
        .merge(routes::ws::routes())
        .merge(SwaggerUi::new("/swagger").url("/openapi.json", ApiDoc::openapi()))
        .with_state(app_state);

    router
}
