use std::usize;

use axum::{
    body::Body,
    extract::{FromRequestParts, Request, State},
    middleware::Next,
    response::Response,
};
use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use jwt::{SignWithKey, VerifyWithKey};
use oauth2::{AuthorizationCode, TokenResponse};
use prefixed_api_key::{PakControllerOsSha256, PrefixedApiKey, PrefixedApiKeyController};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

use crate::{
    AppState, DiscordOAuthClient,
    dtos::{
        auth::{ApiKeyDto, UserToken},
        error::{ApiError, PublicError, ToPublicError},
    },
    entities,
    repositories::{
        AlwaysCloneableConnection, BaseRepository, RepositoryError, users::UserRepositoryExt,
    },
    services::{
        discord_linked_roles::{LinkedRolesService, UserMetadata},
        discord_oauth_tokens::TokenStore,
    },
};

use super::UserId;

const MAX_SKEW_MS: chrono::Duration = chrono::Duration::minutes(5);

#[derive(thiserror::Error, Debug)]
pub enum AuthServiceError {
    #[error("Unexpected Discord API error")]
    DiscordApiError(#[from] serenity::Error),
    #[error("Misconfigured OAuth client")]
    MisconfiguredOAuthClient,
    #[error("Invalid authorization code")]
    InvalidAuthorizationCode,
    #[error("Invalid OAuth request")]
    InvalidOAuthRequest,
    #[error("Invalid scope specified")]
    InvalidScope,
    #[error("Invalid expiration time")]
    InvalidExpirationTime,
    #[error("Missing refresh token")]
    MissingRefreshToken,
    #[error("Other OAuth-related error: {0}")]
    OtherOAuthError(String),
    #[error("Invalid JWT token")]
    InvalidJwtToken,
    #[error("Invalid API key")]
    InvalidApiKey,
    #[error("Invalid HMAC timestamp")]
    InvalidHmacTimestamp,
    #[error("Invalid HMAC signature")]
    InvalidHmacSignature,
    #[error(transparent)]
    Repository(#[from] RepositoryError),
}

impl From<&oauth2::basic::BasicErrorResponseType> for AuthServiceError {
    fn from(value: &oauth2::basic::BasicErrorResponseType) -> Self {
        match value {
            oauth2::basic::BasicErrorResponseType::InvalidClient => {
                AuthServiceError::MisconfiguredOAuthClient
            }
            oauth2::basic::BasicErrorResponseType::InvalidGrant => {
                AuthServiceError::InvalidAuthorizationCode
            }
            oauth2::basic::BasicErrorResponseType::InvalidRequest => {
                AuthServiceError::InvalidOAuthRequest
            }
            oauth2::basic::BasicErrorResponseType::InvalidScope => AuthServiceError::InvalidScope,
            oauth2::basic::BasicErrorResponseType::UnauthorizedClient => {
                AuthServiceError::InvalidOAuthRequest
            }
            oauth2::basic::BasicErrorResponseType::UnsupportedGrantType => {
                AuthServiceError::InvalidOAuthRequest
            }
            oauth2::basic::BasicErrorResponseType::Extension(e) => {
                AuthServiceError::OtherOAuthError(e.to_string())
            }
        }
    }
}

impl ToPublicError for AuthServiceError {
    fn as_public(&self) -> Option<PublicError> {
        match self {
            AuthServiceError::InvalidAuthorizationCode => Some(PublicError::new(
                "invalid_authorization_code",
                "Invalid authorization code",
                StatusCode::FORBIDDEN,
            )),
            AuthServiceError::InvalidJwtToken => Some(PublicError::new(
                "invalid_jwt_token",
                "Invalid JWT token",
                StatusCode::UNAUTHORIZED,
            )),
            AuthServiceError::InvalidApiKey => Some(PublicError::new(
                "invalid_api_key",
                "Invalid API key",
                StatusCode::UNAUTHORIZED,
            )),
            AuthServiceError::InvalidHmacSignature => Some(PublicError::new(
                "invalid_hmac_signature",
                "Invalid HMAC signature",
                StatusCode::UNAUTHORIZED,
            )),
            AuthServiceError::InvalidHmacTimestamp => Some(PublicError::new(
                "invalid_hmac_timestamp",
                "Invalid HMAC timestamp",
                StatusCode::UNAUTHORIZED,
            )),
            _ => None,
        }
    }
}

pub struct AuthService {
    user_repo: BaseRepository<entities::users::Entity>,
    oauth_client: DiscordOAuthClient,
    jwt_secret: Hmac<Sha256>,
    hmac_secret: Hmac<Sha256>,
    http_client: reqwest::Client,
    key_generator: PakControllerOsSha256,
    linked_roles: std::sync::Arc<LinkedRolesService>,
    token_store: std::sync::Arc<TokenStore>,
    db: AlwaysCloneableConnection,
}

impl AuthService {
    pub fn new(
        db: &AlwaysCloneableConnection,
        oauth_client: DiscordOAuthClient,
        jwt_secret: Hmac<Sha256>,
        hmac_secret: Hmac<Sha256>,
        linked_roles: std::sync::Arc<LinkedRolesService>,
        token_store: std::sync::Arc<TokenStore>,
    ) -> Self {
        let http_client = reqwest::ClientBuilder::new()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();

        let key_generator = PrefixedApiKeyController::configure()
            .prefix("ak".to_string())
            .seam_defaults()
            .finalize()
            .unwrap();

        Self {
            user_repo: BaseRepository::new(db),
            oauth_client,
            jwt_secret,
            hmac_secret,
            http_client,
            key_generator,
            linked_roles,
            token_store,
            db: db.clone(),
        }
    }

    pub async fn login_user(&self, code: &str) -> Result<UserToken, AuthServiceError> {
        let token_response = self
            .oauth_client
            .exchange_code(AuthorizationCode::new(code.to_string()))
            .request_async(&self.http_client)
            .await
            .map_err(|error| match error {
                oauth2::RequestTokenError::ServerResponse(response_error) => {
                    response_error.error().into()
                }
                error => AuthServiceError::OtherOAuthError(error.to_string()),
            })?;

        let expiration = chrono::Duration::from_std(
            token_response
                .expires_in()
                .unwrap_or(std::time::Duration::from_secs(0)),
        )
        .map_err(|_| AuthServiceError::InvalidExpirationTime)?;
        let token = token_response.access_token();
        let expires_at = Utc::now() + expiration;

        let discord_client =
            serenity::http::HttpBuilder::new(format!("Bearer {}", token.secret())).build();
        let current_user = discord_client.get_current_user().await?;
        let user_id = current_user.id.get();

        let claims = Claims::new(user_id.into(), expiration);
        let jwt = claims
            .sign(&self.jwt_secret)
            .map_err(|_| AuthServiceError::InvalidJwtToken)?;

        // Persist (encrypted) Discord access + refresh tokens so we can refresh
        // out-of-band (e.g. on /played for linked-roles updates) without
        // needing the user to re-login. Best-effort: a missing refresh token
        // (which Discord only omits if scopes were not granted) or a sealing
        // failure must not break login.
        if let Some(refresh) = token_response.refresh_token() {
            let scopes = token_response
                .scopes()
                .map(|ss| ss.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(" "))
                .unwrap_or_default();
            let access_expires_at = expires_at.naive_utc();
            if let Err(e) = self
                .token_store
                .store(
                    user_id as i64,
                    token.secret(),
                    refresh.secret(),
                    access_expires_at,
                    &scopes,
                )
                .await
            {
                tracing::warn!(user_id, error = ?e, "Failed to persist Discord OAuth tokens");
            }
        } else {
            tracing::warn!(
                user_id,
                "Discord did not return a refresh token; auto-refresh will not work for this user"
            );
        }

        // Best-effort: pull Discord connections (requires `connections` scope on
        // the original auth URL) and persist any YouTube channels. Failures
        // here must not break login — they only mean the user didn't grant the
        // scope or Discord is having a moment.
        match self.fetch_discord_connections(token.secret()).await {
            Ok(conns) => {
                for conn in conns.into_iter().filter(|c| c.r#type == "youtube") {
                    if let Err(e) = self
                        .user_repo
                        .upsert_youtube_account(user_id as i64, &conn.id, &conn.name)
                        .await
                    {
                        tracing::warn!(
                            user_id,
                            error = ?e,
                            "Failed to upsert YouTube connection"
                        );
                    }
                }
            }
            Err(e) => {
                tracing::warn!(user_id, error = ?e, "Failed to fetch Discord connections");
            }
        }

        // Best-effort: push linked-role metadata. Requires the user-scope
        // `role_connections.write`. Failures only mean the scope wasn't
        // granted or Discord rejected it; login still succeeds.
        if let Err(e) = self
            .push_linked_role_metadata(user_id as i64, token.secret())
            .await
        {
            tracing::warn!(user_id, error = ?e, "Failed to push linked-role metadata");
        }

        Ok(UserToken {
            token: jwt,
            user_id,
            expires_in: expiration.num_seconds() as u64,
            expires_at: expires_at.timestamp() as u64,
        })
    }

    /// Build a [`UserMetadata`] snapshot from the current DB state and push
    /// it via the [`LinkedRolesService`].
    async fn push_linked_role_metadata(
        &self,
        user_id: i64,
        access_token: &str,
    ) -> Result<(), AuthServiceError> {
        use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter};

        let user = entities::users::Entity::find_by_id(user_id)
            .one(&*self.db)
            .await
            .map_err(|e| AuthServiceError::OtherOAuthError(e.to_string()))?
            .ok_or(AuthServiceError::OtherOAuthError(
                "User row missing after creation".into(),
            ))?;
        let can_count = entities::cans::Entity::find()
            .filter(entities::cans::Column::AddedBy.eq(user_id))
            .count(&*self.db)
            .await
            .map_err(|e| AuthServiceError::OtherOAuthError(e.to_string()))?;

        let listening_hours = (user.watched_time / 3600) as i32;
        let metadata = UserMetadata {
            listening_hours,
            can_count: can_count.try_into().unwrap_or(i32::MAX),
            boonbucks: user.boonbucks,
        };

        self.linked_roles
            .push_for_user(user_id, access_token, &metadata)
            .await
            .map_err(|e| AuthServiceError::OtherOAuthError(e.to_string()))
    }

    async fn fetch_discord_connections(
        &self,
        access_token: &str,
    ) -> Result<Vec<DiscordConnection>, AuthServiceError> {
        let response = self
            .http_client
            .get("https://discord.com/api/v10/users/@me/connections")
            .bearer_auth(access_token)
            .send()
            .await
            .map_err(|e| AuthServiceError::OtherOAuthError(e.to_string()))?;

        if !response.status().is_success() {
            return Err(AuthServiceError::OtherOAuthError(format!(
                "Discord connections endpoint returned {}",
                response.status()
            )));
        }

        response
            .json::<Vec<DiscordConnection>>()
            .await
            .map_err(|e| AuthServiceError::OtherOAuthError(e.to_string()))
    }

    pub fn verify_token(&self, token: &str) -> Result<Claims, AuthServiceError> {
        Claims::verify(token, &self.jwt_secret).map_err(|_| AuthServiceError::InvalidJwtToken)
    }

    pub async fn create_api_key(
        &self,
        user_id: UserId,
        description: &str,
    ) -> Result<ApiKeyDto, AuthServiceError> {
        let (pak, hash) = self.key_generator.generate_key_and_hash();

        let key = self
            .user_repo
            .create_api_key(user_id.into(), pak.short_token(), &hash, description)
            .await?;

        Ok(ApiKeyDto {
            api_key: pak.to_string(),
            description: key.description,
        })
    }

    pub async fn check_api_key(&self, api_key: &str) -> Result<i64, AuthServiceError> {
        let pak: PrefixedApiKey = api_key
            .try_into()
            .map_err(|_| AuthServiceError::InvalidApiKey)?;
        let user = self.user_repo.find_by_api_key(pak.short_token()).await?;

        let Some((key, user)) = user else {
            return Err(AuthServiceError::InvalidApiKey);
        };

        if self.key_generator.check_hash(&pak, &key.hash) {
            Ok(user.id)
        } else {
            Err(AuthServiceError::InvalidApiKey)
        }
    }

    pub fn verify_hmac(
        &self,
        body: &[u8],
        signature: &[u8],
        timestamp: &str,
        method: &str,
        path: &str,
    ) -> Result<(), AuthServiceError> {
        let sent_at = match DateTime::parse_from_rfc3339(timestamp) {
            Ok(dt) => dt.with_timezone(&Utc),
            Err(_) => return Err(AuthServiceError::InvalidHmacTimestamp),
        };

        let now = Utc::now();
        if (now - sent_at).num_seconds().abs() > MAX_SKEW_MS.num_seconds() {
            return Err(AuthServiceError::InvalidHmacTimestamp);
        }

        let body_hash = hex::encode(Sha256::digest(&body));
        let canonical = format!("{}\n{}\n{}\n{}", method, path, body_hash, timestamp);

        let mut mac = self.hmac_secret.clone();
        mac.update(canonical.as_bytes());
        let expected = mac.finalize().into_bytes();

        let supplied = match hex::decode(signature) {
            Ok(bytes) => bytes,
            Err(_) => return Err(AuthServiceError::InvalidHmacSignature),
        };
        if supplied.len() != expected.len() || supplied.ct_eq(&expected).unwrap_u8() == 0 {
            return Err(AuthServiceError::InvalidHmacSignature);
        }

        Ok(())
    }
}

/// Subset of the Discord [Connection](https://discord.com/developers/docs/resources/user#connection-object) object that we need.
#[derive(Deserialize, Debug, Clone)]
struct DiscordConnection {
    /// Connection's account id (e.g. YouTube channel id).
    id: String,
    /// Display name of the linked account.
    name: String,
    /// Connection type — `"youtube"`, `"twitch"`, etc.
    r#type: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Claims {
    #[serde(flatten)]
    standard: jwt::RegisteredClaims,
}

impl Claims {
    pub fn new(user_id: UserId, expiry: chrono::Duration) -> Self {
        let now = chrono::Utc::now();
        let expiry = now + expiry;

        Self {
            standard: jwt::RegisteredClaims {
                issuer: Some("caliborn".to_string()),
                subject: Some(user_id.to_string()),
                audience: Some("calliope".to_string()),
                expiration: Some(expiry.timestamp() as u64),
                not_before: Some(now.timestamp() as u64),
                issued_at: Some(now.timestamp() as u64),
                json_web_token_id: None,
            },
        }
    }

    pub fn sign(&self, secret: &Hmac<Sha256>) -> Result<String, jwt::Error> {
        self.sign_with_key(secret)
    }

    pub fn verify(token: &str, secret: &Hmac<Sha256>) -> Result<Self, jwt::Error> {
        token.verify_with_key(secret)
    }

    pub fn is_valid_user(&self) -> bool {
        self.standard.subject.is_some()
            && self.standard.audience == Some("calliope".to_string())
            && self.valid()
    }

    pub fn valid(&self) -> bool {
        let now = chrono::Utc::now();

        if let Some(nbf) = self.standard.not_before.as_ref() {
            if *nbf > now.timestamp() as u64 {
                return false;
            }
        }

        if let Some(exp) = self.standard.expiration.as_ref() {
            if *exp < now.timestamp() as u64 {
                return false;
            }
        }

        true
    }
}

pub struct AuthenticatedUser(pub Actor);

impl<S: Send + Sync> FromRequestParts<S> for AuthenticatedUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        let extension =
            parts
                .extensions
                .get::<Actor>()
                .ok_or(ApiError::Internal(anyhow::anyhow!(
                    "missing actor extension (perhaps middleware is not used)"
                )))?;
        Ok(AuthenticatedUser(extension.clone()))
    }
}

#[derive(Clone)]
pub enum Actor {
    User { user_id: UserId },
    Bot { user_id: UserId },
}

impl Actor {
    pub fn user_id(&self) -> UserId {
        match self {
            Actor::User { user_id } => *user_id,
            Actor::Bot { user_id } => *user_id,
        }
    }
}

pub async fn authenticate(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    if let (Some(signature), Some(timestamp), Some(user_id)) = (
        request
            .headers()
            .get("X-Caliborn-Signature")
            .map(|s| s.as_bytes().to_vec()),
        request
            .headers()
            .get("X-Caliborn-Timestamp")
            .map(|s| s.to_str().unwrap_or_default().to_string()),
        request
            .headers()
            .get("X-Caliborn-User-Id")
            .map(|s| s.to_str().unwrap_or_default().to_string()),
    ) {
        let user_id = user_id.parse::<u64>().map_err(|_| {
            ApiError::Public(PublicError::new(
                "missing_user_id_header",
                "The X-Caliborn-User-Id header is not set",
                StatusCode::UNAUTHORIZED,
            ))
        })?;

        let (parts, body) = request.into_parts();
        let method = parts.method.as_str();
        let bytes = axum::body::to_bytes(body, usize::MAX)
            .await
            .map_err(|_| ApiError::Internal(anyhow::anyhow!("reading body failed")))?;
        state.service_registry.auth_service().verify_hmac(
            &bytes,
            &signature,
            &timestamp,
            method,
            parts.uri.path(),
        )?;

        let mut req = Request::from_parts(parts, Body::from(bytes));
        req.extensions_mut().insert(Actor::Bot {
            user_id: user_id.into(),
        });
        return Ok(next.run(req).await);
    }

    if let Some(auth) = request.headers().get(axum::http::header::AUTHORIZATION) {
        let token = auth.to_str().unwrap_or_default().to_string();
        let stripped = token
            .strip_prefix("Bearer ")
            .ok_or(ApiError::Public(PublicError::new(
                "invalid-auth-header",
                "Invalid authentication header",
                StatusCode::UNAUTHORIZED,
            )))?;

        if stripped.starts_with("ak_") {
            let user_id = state
                .service_registry
                .auth_service()
                .check_api_key(stripped)
                .await?;
            request.extensions_mut().insert(Actor::User {
                user_id: user_id.into(),
            });
            return Ok(next.run(request).await);
        } else {
            let claims = state
                .service_registry
                .auth_service()
                .verify_token(&stripped)?;
            if !claims.is_valid_user() {
                return Err(ApiError::Public(PublicError::new(
                    "invalid-auth-header",
                    "Invalid authentication header",
                    StatusCode::UNAUTHORIZED,
                )));
            }

            let user_id = {
                let Some(sub) = claims.standard.subject.as_ref() else {
                    return Err(ApiError::Public(PublicError::new(
                        "invalid-auth-header",
                        "Invalid authentication header",
                        StatusCode::UNAUTHORIZED,
                    )));
                };
                sub.parse::<u64>().map_err(|_| {
                    ApiError::Public(PublicError::new(
                        "invalid-auth-header",
                        "Invalid authentication header",
                        StatusCode::UNAUTHORIZED,
                    ))
                })?
            };
            request.extensions_mut().insert(Actor::User {
                user_id: user_id.into(),
            });
            return Ok(next.run(request).await);
        }
    }

    Err(ApiError::Public(PublicError::new(
        "invalid-auth-header",
        "Invalid authentication header",
        StatusCode::UNAUTHORIZED,
    )))
}
