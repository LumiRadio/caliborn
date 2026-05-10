//! Authentication data transfer objects for the Caliborn API.
//!
//! This module contains DTOs related to authentication, including user tokens,
//! API keys, and login requests used in the authentication flow.
//!
//! # Examples
//!
//! ```rust
//! # use caliborn::dtos::auth::UserToken;
//! let token = UserToken {
//!     token: String::from("jwt-token-here"),
//!     user_id: 675674657,
//!     expires_in: 3600,
//!     expires_at: 1682384222,
//! };
//! ```

use axum::response::IntoResponse;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use super::json;

/// A response to a successful login via Discord.
///
/// Contains an access token which can be used to authenticate to other endpoints
/// in the API. The token is a JWT with a limited lifetime.
///
/// # Examples
///
/// ```rust
/// # use caliborn::dtos::auth::UserToken;
/// let token = UserToken {
///     token: String::from("eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9..."),
///     user_id: 675674657,
///     expires_in: 3600,
///     expires_at: 1682384222,
/// };
/// ```
#[derive(Serialize, ToSchema)]
#[schema(
    description = "A response to a successful login via Discord",
    examples(
        json!({
            "token": "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJ1c2VyX2lkIjo2NzU3NjQ3NjUsImlhdCI6MTY4MjM3NDQyMiwiZXhwIjoxNjgyMzgyNDIyfQ.QUQ0z3O9TqYUv4JzL9Vq7Z0T2Zr4xJnNzK4yRfGpPZ",
            "user_id": 675674657,
            "expires_in": 3600,
            "expires_at": 1682384222
        })
    )
)]
pub struct UserToken {
    /// An access token which can be used to authenticate to other endpoints.
    /// This is a JWT token containing the user's identity information.
    #[schema(examples(
        "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJ1c2VyX2lkIjo2NzU3NjQ3NjUsImlhdCI6MTY4MjM3NDQyMiwiZXhwIjoxNjgyMzgyNDIyfQ.QUQ0z3O9TqYUv4JzL9Vq7Z0T2Zr4xJnNzK4yRfGpPZ"
    ))]
    pub token: String,
    /// The ID of the user who was logged in.
    #[schema(examples(675674657))]
    pub user_id: u64,
    /// The time in seconds until the access token expires.
    #[schema(examples(3600))]
    pub expires_in: u64,
    /// The timestamp when the access token expires.
    #[schema(examples(1682384222))]
    pub expires_at: u64,
}

impl IntoResponse for UserToken {
    fn into_response(self) -> axum::response::Response {
        json(self).into_response()
    }
}

/// Represents an API key with its full value and description.
///
/// This DTO is used when returning API key information to the client,
/// particularly after creating a new API key. Note that the full API key
/// is only returned once upon creation and cannot be retrieved again.
///
/// # Examples
///
/// ```rust
/// # use caliborn::dtos::auth::ApiKeyDto;
/// let api_key = ApiKeyDto {
///     api_key: String::from("ak_BRTRKFsL_51FwqftsmMDHHbJAMEXXHCgG"),
///     description: String::from("My API key"),
/// };
/// ```
///
/// # Security
///
/// The full API key value should only be returned once when it is created.
/// After that, only a masked version should be shown to users.
#[derive(Serialize, ToSchema)]
#[schema(
    description = "An API key",
    examples(
        json!({
            "api_key": "ak_BRTRKFsL_51FwqftsmMDHHbJAMEXXHCgG",
            "description": "My API key"
        })
    )
)]
pub struct ApiKeyDto {
    /// The API key value in its complete form.
    #[schema(examples("ak_BRTRKFsL_51FwqftsmMDHHbJAMEXXHCgG"))]
    pub api_key: String,
    /// A user-provided description of the API key.
    #[schema(examples("My API key"))]
    pub description: String,
}

/// Query parameters for the GET /auth/discord/login endpoint.
///
/// Carries the Discord OAuth2 authorization code returned by Discord's
/// authorization-code flow.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct DiscordLoginQuery {
    /// The authorization code received from Discord after the user has authorized your application.
    #[param(example = "NhhvTDYsFcdgNLnnLijcl7Ku7bEEeee")]
    pub code: String,
}
