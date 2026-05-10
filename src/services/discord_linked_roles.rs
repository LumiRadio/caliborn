//! Discord [Linked Roles](https://discord.com/developers/docs/tutorials/configuring-app-metadata-for-linked-roles)
//! integration.
//!
//! Three flows:
//!
//! 1. **One-time metadata registration** — `caliborn linked-roles register`
//!    PUTs the role-connection metadata schema for the application using a
//!    bot token. Run this once per app, or whenever the schema changes.
//!
//! 2. **Per-user push on login** — after Discord OAuth login (with the
//!    user-scope `role_connections.write`), push the current user's metadata
//!    values so Discord can evaluate role-connection rules.
//!
//! 3. **Out-of-band push** — `/playback/played` ingest and the manual
//!    `POST /user/me/sync-linked-role` endpoint use the encrypted refresh
//!    token (stored at login by Phase 8.5) to mint a fresh access token via
//!    [`crate::services::discord_oauth_tokens::TokenStore`] and push
//!    updated metadata without requiring the user to re-login.

use std::sync::Arc;

use sea_orm::{ActiveModelTrait, ActiveValue, EntityTrait, Set};
use serde::{Deserialize, Serialize};

use crate::{entities, repositories::AlwaysCloneableConnection};

const METADATA_TYPE_INTEGER_GTE: u8 = 2;

#[derive(thiserror::Error, Debug)]
pub enum LinkedRolesError {
    #[error("Discord API error: {status} {body}")]
    Discord { status: u16, body: String },
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error(transparent)]
    Db(#[from] sea_orm::DbErr),
}

/// One entry in the application's role-connection metadata schema.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MetadataField {
    pub key: String,
    pub name: String,
    pub description: String,
    #[serde(rename = "type")]
    pub type_: u8,
}

/// The default schema Caliborn registers. Add new fields here when the
/// frontend wants to gate roles on them.
pub fn default_schema() -> Vec<MetadataField> {
    vec![
        MetadataField {
            key: "listening_hours".into(),
            name: "Listening Hours".into(),
            description: "Total hours listened to LumiRadio.".into(),
            type_: METADATA_TYPE_INTEGER_GTE,
        },
        MetadataField {
            key: "can_count".into(),
            name: "Cans Added".into(),
            description: "Number of cans added to Can Town.".into(),
            type_: METADATA_TYPE_INTEGER_GTE,
        },
        MetadataField {
            key: "boonbucks".into(),
            name: "Boonbucks".into(),
            description: "Boonbucks balance.".into(),
            type_: METADATA_TYPE_INTEGER_GTE,
        },
    ]
}

/// Per-user metadata payload pushed to Discord.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct UserMetadata {
    pub listening_hours: i32,
    pub can_count: i32,
    pub boonbucks: i32,
}

#[derive(Serialize)]
struct PushBody<'a> {
    platform_name: &'a str,
    metadata: &'a UserMetadata,
}

#[derive(Clone)]
pub struct LinkedRolesService {
    http_client: reqwest::Client,
    application_id: String,
    platform_name: String,
    db: AlwaysCloneableConnection,
}

impl LinkedRolesService {
    pub fn new(
        http_client: reqwest::Client,
        application_id: String,
        platform_name: String,
        db: AlwaysCloneableConnection,
    ) -> Self {
        Self {
            http_client,
            application_id,
            platform_name,
            db,
        }
    }

    pub fn application_id(&self) -> &str {
        &self.application_id
    }

    /// One-time PUT of the metadata schema. Authenticated with the
    /// application's **bot** token (`Bot <token>`).
    pub async fn register_metadata(
        &self,
        bot_token: &str,
        schema: &[MetadataField],
    ) -> Result<(), LinkedRolesError> {
        let url = format!(
            "https://discord.com/api/v10/applications/{}/role-connections/metadata",
            self.application_id
        );
        let response = self
            .http_client
            .put(&url)
            .header("Authorization", format!("Bot {}", bot_token))
            .json(schema)
            .send()
            .await?;
        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            return Err(LinkedRolesError::Discord { status, body });
        }
        Ok(())
    }

    /// PUT the user's metadata via their OAuth `access_token` (must carry the
    /// `role_connections.write` scope). Records the push in
    /// `discord_role_connections`.
    pub async fn push_for_user(
        &self,
        user_id: i64,
        access_token: &str,
        metadata: &UserMetadata,
    ) -> Result<(), LinkedRolesError> {
        let url = format!(
            "https://discord.com/api/v10/users/@me/applications/{}/role-connection",
            self.application_id
        );
        let body = PushBody {
            platform_name: &self.platform_name,
            metadata,
        };
        let response = self
            .http_client
            .put(&url)
            .bearer_auth(access_token)
            .json(&body)
            .send()
            .await?;
        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            return Err(LinkedRolesError::Discord { status, body });
        }

        // Record the snapshot so future per-played debounce knows what was
        // last pushed. Upsert by user_id PK.
        let now = chrono::Utc::now().naive_utc();
        let existing = entities::discord_role_connections::Entity::find_by_id(user_id)
            .one(&*self.db)
            .await?;
        if existing.is_some() {
            entities::discord_role_connections::Entity::update(
                entities::discord_role_connections::ActiveModel {
                    user_id: ActiveValue::unchanged(user_id),
                    last_pushed_at: Set(Some(now)),
                    listening_hours_snapshot: Set(Some(metadata.listening_hours)),
                    can_count_snapshot: Set(Some(metadata.can_count)),
                    boonbucks_snapshot: Set(Some(metadata.boonbucks)),
                },
            )
            .exec(&*self.db)
            .await?;
        } else {
            entities::discord_role_connections::ActiveModel {
                user_id: Set(user_id),
                last_pushed_at: Set(Some(now)),
                listening_hours_snapshot: Set(Some(metadata.listening_hours)),
                can_count_snapshot: Set(Some(metadata.can_count)),
                boonbucks_snapshot: Set(Some(metadata.boonbucks)),
            }
            .insert(&*self.db)
            .await?;
        }
        Ok(())
    }
}

/// Build a fresh [`UserMetadata`] snapshot from the database for `user_id`.
pub async fn build_metadata(
    db: &AlwaysCloneableConnection,
    user_id: i64,
) -> Result<UserMetadata, sea_orm::DbErr> {
    use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter};

    let user = entities::users::Entity::find_by_id(user_id)
        .one(&**db)
        .await?
        .ok_or(sea_orm::DbErr::RecordNotFound(format!("users[{user_id}]")))?;
    let can_count = entities::cans::Entity::find()
        .filter(entities::cans::Column::AddedBy.eq(user_id))
        .count(&**db)
        .await?;
    Ok(UserMetadata {
        listening_hours: (user.watched_time / 3600) as i32,
        can_count: can_count.try_into().unwrap_or(i32::MAX),
        boonbucks: user.boonbucks,
    })
}

/// Returns true if `user_id`'s linked-role snapshot is older than `min_age`
/// or has never been pushed. Used by `/played` to debounce re-pushes.
pub async fn should_push_after(
    db: &AlwaysCloneableConnection,
    user_id: i64,
    min_age: chrono::Duration,
) -> Result<bool, sea_orm::DbErr> {
    let row = entities::discord_role_connections::Entity::find_by_id(user_id)
        .one(&**db)
        .await?;
    Ok(match row.and_then(|r| r.last_pushed_at) {
        None => true,
        Some(last) => chrono::Utc::now().naive_utc() - last >= min_age,
    })
}

/// Boxed helper used by the registry's cached service slot.
pub type SharedLinkedRolesService = Arc<LinkedRolesService>;
