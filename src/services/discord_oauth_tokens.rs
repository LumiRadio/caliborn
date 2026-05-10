//! Encrypted Discord OAuth token storage with auto-refresh.

use std::sync::Arc;

use chrono::{Duration, NaiveDateTime, Utc};
use oauth2::{RefreshToken, TokenResponse};
use sea_orm::{ActiveModelTrait, ActiveValue, EntityTrait, Set};

use crate::{
    DiscordOAuthClient, RepositoryError, entities,
    repositories::AlwaysCloneableConnection,
    services::secrets::{SealerError, TokenSealer},
};

#[derive(thiserror::Error, Debug)]
pub enum TokenStoreError {
    #[error(transparent)]
    Sealer(#[from] SealerError),
    #[error(transparent)]
    Db(#[from] sea_orm::DbErr),
    #[error(transparent)]
    Repository(#[from] RepositoryError),
    #[error("No stored tokens for user {0}")]
    NotFound(i64),
    #[error("OAuth refresh failed: {0}")]
    RefreshFailed(String),
    #[error("Stored token contains invalid UTF-8")]
    InvalidUtf8,
}

#[derive(Debug, Clone)]
pub struct StoredTokens {
    pub access_token: String,
    pub refresh_token: String,
    pub access_expires_at: NaiveDateTime,
    pub scopes: String,
}

#[derive(Clone)]
pub struct TokenStore {
    db: AlwaysCloneableConnection,
    sealer: Arc<TokenSealer>,
    oauth_client: DiscordOAuthClient,
    http_client: reqwest::Client,
}

impl TokenStore {
    pub fn new(
        db: AlwaysCloneableConnection,
        sealer: Arc<TokenSealer>,
        oauth_client: DiscordOAuthClient,
        http_client: reqwest::Client,
    ) -> Self {
        Self {
            db,
            sealer,
            oauth_client,
            http_client,
        }
    }

    /// Encrypt and persist a fresh `(access, refresh, expires_at, scopes)`
    /// tuple. Upserts on `user_id`.
    pub async fn store(
        &self,
        user_id: i64,
        access_token: &str,
        refresh_token: &str,
        access_expires_at: NaiveDateTime,
        scopes: &str,
    ) -> Result<(), TokenStoreError> {
        let (access_ct, access_nonce) = self.sealer.seal(access_token.as_bytes())?;
        let (refresh_ct, refresh_nonce) = self.sealer.seal(refresh_token.as_bytes())?;
        let now = Utc::now().naive_utc();

        let existing = entities::discord_oauth_tokens::Entity::find_by_id(user_id)
            .one(&*self.db)
            .await?;
        if existing.is_some() {
            entities::discord_oauth_tokens::Entity::update(
                entities::discord_oauth_tokens::ActiveModel {
                    user_id: ActiveValue::unchanged(user_id),
                    access_token_ciphertext: Set(access_ct),
                    access_token_nonce: Set(access_nonce.to_vec()),
                    refresh_token_ciphertext: Set(refresh_ct),
                    refresh_token_nonce: Set(refresh_nonce.to_vec()),
                    access_expires_at: Set(access_expires_at),
                    scopes: Set(scopes.to_string()),
                    updated_at: Set(now),
                },
            )
            .exec(&*self.db)
            .await?;
        } else {
            entities::discord_oauth_tokens::ActiveModel {
                user_id: Set(user_id),
                access_token_ciphertext: Set(access_ct),
                access_token_nonce: Set(access_nonce.to_vec()),
                refresh_token_ciphertext: Set(refresh_ct),
                refresh_token_nonce: Set(refresh_nonce.to_vec()),
                access_expires_at: Set(access_expires_at),
                scopes: Set(scopes.to_string()),
                updated_at: Set(now),
            }
            .insert(&*self.db)
            .await?;
        }
        Ok(())
    }

    /// Fetch and decrypt the stored tuple. Returns `NotFound` if no row.
    pub async fn fetch(&self, user_id: i64) -> Result<StoredTokens, TokenStoreError> {
        let row = entities::discord_oauth_tokens::Entity::find_by_id(user_id)
            .one(&*self.db)
            .await?
            .ok_or(TokenStoreError::NotFound(user_id))?;
        let access_bytes = self
            .sealer
            .unseal(&row.access_token_ciphertext, &row.access_token_nonce)?;
        let refresh_bytes = self
            .sealer
            .unseal(&row.refresh_token_ciphertext, &row.refresh_token_nonce)?;
        Ok(StoredTokens {
            access_token: String::from_utf8(access_bytes)
                .map_err(|_| TokenStoreError::InvalidUtf8)?,
            refresh_token: String::from_utf8(refresh_bytes)
                .map_err(|_| TokenStoreError::InvalidUtf8)?,
            access_expires_at: row.access_expires_at,
            scopes: row.scopes,
        })
    }

    /// Return a still-valid Discord access token for `user_id`, refreshing
    /// via Discord's token endpoint if the stored access token has expired
    /// (or is within `skew`).
    pub async fn valid_access_token(&self, user_id: i64) -> Result<String, TokenStoreError> {
        const SKEW: Duration = Duration::seconds(60);
        let stored = self.fetch(user_id).await?;
        if Utc::now().naive_utc() + SKEW < stored.access_expires_at {
            return Ok(stored.access_token);
        }

        // Refresh.
        let token_response = self
            .oauth_client
            .exchange_refresh_token(&RefreshToken::new(stored.refresh_token.clone()))
            .request_async(&self.http_client)
            .await
            .map_err(|e| TokenStoreError::RefreshFailed(e.to_string()))?;

        let new_access = token_response.access_token().secret().to_string();
        let new_refresh = token_response
            .refresh_token()
            .map(|r| r.secret().to_string())
            .unwrap_or(stored.refresh_token);
        let lifetime = token_response
            .expires_in()
            .map(|d| Duration::from_std(d).unwrap_or(Duration::seconds(0)))
            .unwrap_or(Duration::seconds(0));
        let new_expires_at = Utc::now().naive_utc() + lifetime;

        self.store(
            user_id,
            &new_access,
            &new_refresh,
            new_expires_at,
            &stored.scopes,
        )
        .await?;
        Ok(new_access)
    }
}
