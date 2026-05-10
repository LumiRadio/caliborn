//! Streamlabs Chatbot legacy data import + Discord-user matching.
//!
//! - **Import** loads a Streamlabs JSON dump into the `slcb_currency` table
//!   (idempotent: rows matching `(username, user_id)` are updated in place).
//! - **Match** walks `connected_youtube_accounts` and, for any user whose
//!   linked YouTube channel id matches an `slcb_currency.user_id`, adds the
//!   stored `hours*3600` to `users.watched_time` and `points` to
//!   `users.boonbucks`, then marks the user `migrated = true` so re-runs
//!   are no-ops.
//!
//! Plan note: `slcb_*` tables are kept permanently — late-joining Discord
//! users may match SLCB rows imported years prior.

use std::path::Path;

use sea_orm::{ActiveModelTrait, ActiveValue, ColumnTrait, EntityTrait, QueryFilter, Set};
use serde::Deserialize;
use utoipa::ToSchema;

use crate::{entities, repositories::AlwaysCloneableConnection};

#[derive(thiserror::Error, Debug)]
pub enum SlcbError {
    #[error("Failed to read JSON file: {0}")]
    Io(#[from] std::io::Error),
    #[error("Failed to parse Streamlabs JSON: {0}")]
    Parse(#[from] serde_json::Error),
    #[error(transparent)]
    Db(#[from] sea_orm::DbErr),
    #[error("User `{0}` not found")]
    UserNotFound(i64),
    #[error("SLCB username `{0}` not found")]
    SlcbUsernameNotFound(String),
    #[error("User `{0}` is already linked to SLCB data (migrated). Use force=true to override.")]
    UserAlreadyMigrated(i64),
}

impl crate::dtos::error::ToPublicError for SlcbError {
    fn as_public(&self) -> Option<crate::dtos::error::PublicError> {
        use crate::dtos::error::PublicError;
        use reqwest::StatusCode;
        match self {
            SlcbError::UserNotFound(_) | SlcbError::SlcbUsernameNotFound(_) => Some(
                PublicError::with_owned("not-found", self.to_string(), StatusCode::NOT_FOUND),
            ),
            SlcbError::UserAlreadyMigrated(_) => Some(PublicError::with_owned(
                "user-already-migrated",
                self.to_string(),
                StatusCode::CONFLICT,
            )),
            SlcbError::Io(_) | SlcbError::Parse(_) | SlcbError::Db(_) => None,
        }
    }
}

/// One row from a Streamlabs Chatbot export. Fields use serde aliases so the
/// loader accepts both the modern (`Name`, `UserID`, `Hours`, `Points`) and
/// snake-case shapes commonly seen in older exports.
#[derive(Deserialize, Debug, Clone, ToSchema)]
pub struct StreamlabsRecord {
    #[serde(alias = "Name", alias = "username", alias = "name")]
    pub username: String,
    #[serde(default, alias = "UserID", alias = "user_id", alias = "userid")]
    pub user_id: Option<String>,
    #[serde(default, alias = "Hours", alias = "hours")]
    pub hours: i32,
    #[serde(default, alias = "Points", alias = "points")]
    pub points: i32,
}

#[derive(Debug, Default, Clone)]
pub struct ImportSummary {
    pub inserted: u64,
    pub updated: u64,
    pub skipped: u64,
}

#[derive(Debug, Default, Clone)]
pub struct MatchSummary {
    pub considered: u64,
    pub matched: u64,
    pub already_migrated: u64,
    pub no_slcb_row: u64,
}

/// Parse a Streamlabs JSON file into an upsert plan.
pub fn parse_streamlabs<P: AsRef<Path>>(path: P) -> Result<Vec<StreamlabsRecord>, SlcbError> {
    let bytes = std::fs::read(path)?;
    Ok(serde_json::from_slice(&bytes)?)
}

/// Upsert the supplied records into `slcb_currency`. Match key is
/// `(username, user_id)`; same key + new values updates the row in place.
pub async fn import_records(
    db: &AlwaysCloneableConnection,
    records: &[StreamlabsRecord],
    dry_run: bool,
) -> Result<ImportSummary, SlcbError> {
    let mut summary = ImportSummary::default();

    for record in records {
        let mut q = entities::slcb_currency::Entity::find()
            .filter(entities::slcb_currency::Column::Username.eq(&record.username));
        q = match &record.user_id {
            Some(uid) => q.filter(entities::slcb_currency::Column::UserId.eq(uid.as_str())),
            None => q.filter(entities::slcb_currency::Column::UserId.is_null()),
        };
        let existing = q.one(&**db).await?;

        if dry_run {
            if existing.is_some() {
                summary.updated += 1;
            } else {
                summary.inserted += 1;
            }
            continue;
        }

        match existing {
            Some(row) => {
                let mut active: entities::slcb_currency::ActiveModel = row.into();
                active.points = Set(record.points);
                active.hours = Set(record.hours);
                active.update(&**db).await?;
                summary.updated += 1;
            }
            None => {
                entities::slcb_currency::ActiveModel {
                    username: ActiveValue::set(record.username.clone()),
                    user_id: ActiveValue::set(record.user_id.clone()),
                    points: ActiveValue::set(record.points),
                    hours: ActiveValue::set(record.hours),
                    ..Default::default()
                }
                .insert(&**db)
                .await?;
                summary.inserted += 1;
            }
        }
    }

    Ok(summary)
}

/// Match a single user's linked YouTube channels against `slcb_currency`
/// and credit any unmigrated SLCB row to the user. Idempotent — once the
/// user is `migrated = true`, this is a no-op.
///
/// Used by the login flow to auto-trigger SLCB matching the moment a user
/// links Discord+YouTube, without requiring the operator to run
/// `caliborn match-slcb` manually.
pub async fn match_for_user(
    db: &AlwaysCloneableConnection,
    user_id: i64,
) -> Result<MatchSummary, SlcbError> {
    let mut summary = MatchSummary::default();

    let user = entities::users::Entity::find_by_id(user_id)
        .one(&**db)
        .await?;
    let Some(user) = user else {
        summary.no_slcb_row += 1;
        return Ok(summary);
    };
    if user.migrated {
        summary.already_migrated += 1;
        return Ok(summary);
    }

    let links = entities::connected_youtube_accounts::Entity::find()
        .filter(entities::connected_youtube_accounts::Column::UserId.eq(user_id))
        .all(&**db)
        .await?;

    for link in links {
        summary.considered += 1;

        let slcb = entities::slcb_currency::Entity::find()
            .filter(entities::slcb_currency::Column::UserId.eq(link.youtube_channel_id.as_str()))
            .one(&**db)
            .await?;
        let Some(slcb) = slcb else {
            summary.no_slcb_row += 1;
            continue;
        };

        let new_watched = user.watched_time + (slcb.hours as i64) * 3600;
        let new_boonbucks = user.boonbucks + slcb.points;
        entities::users::Entity::update(entities::users::ActiveModel {
            id: ActiveValue::unchanged(user.id),
            watched_time: Set(new_watched),
            boonbucks: Set(new_boonbucks),
            migrated: Set(true),
            ..Default::default()
        })
        .exec(&**db)
        .await?;

        summary.matched += 1;
        tracing::info!(
            user_id = user.id,
            channel = %link.youtube_channel_id,
            slcb_username = %slcb.username,
            hours = slcb.hours,
            points = slcb.points,
            "imported SLCB data for user (auto-trigger)"
        );
        // First match wins — once migrated, subsequent links are no-ops.
        break;
    }

    Ok(summary)
}

/// Result of [`link_user_to_slcb_username`].
#[derive(Debug, Clone)]
pub struct ForcedLinkSummary {
    pub user_id: i64,
    pub slcb_username: String,
    pub hours_credited: i32,
    pub points_credited: i32,
    pub watched_time_after: i64,
    pub boonbucks_after: i32,
}

/// Force-link a Caliborn user to the SLCB row with the given username
/// (case-insensitive). Credits `hours*3600` to `users.watched_time` and
/// `points` to `users.boonbucks`, then marks the user `migrated = true`.
///
/// If the user is already `migrated = true`, returns
/// [`SlcbError::UserAlreadyMigrated`] unless `force` is true. The credit is
/// always applied additively — caller should refuse `force` after careful
/// review.
pub async fn link_user_to_slcb_username(
    db: &AlwaysCloneableConnection,
    user_id: i64,
    slcb_username: &str,
    force: bool,
) -> Result<ForcedLinkSummary, SlcbError> {
    let user = entities::users::Entity::find_by_id(user_id)
        .one(&**db)
        .await?
        .ok_or(SlcbError::UserNotFound(user_id))?;

    if user.migrated && !force {
        return Err(SlcbError::UserAlreadyMigrated(user_id));
    }

    let slcb = entities::slcb_currency::Entity::find()
        .filter(
            sea_orm::sea_query::Expr::expr(sea_orm::sea_query::Func::lower(
                sea_orm::sea_query::Expr::col(entities::slcb_currency::Column::Username),
            ))
            .eq(slcb_username.to_lowercase()),
        )
        .one(&**db)
        .await?
        .ok_or_else(|| SlcbError::SlcbUsernameNotFound(slcb_username.to_string()))?;

    let new_watched = user.watched_time + (slcb.hours as i64) * 3600;
    let new_boonbucks = user.boonbucks + slcb.points;

    entities::users::Entity::update(entities::users::ActiveModel {
        id: ActiveValue::unchanged(user.id),
        watched_time: Set(new_watched),
        boonbucks: Set(new_boonbucks),
        migrated: Set(true),
        ..Default::default()
    })
    .exec(&**db)
    .await?;

    tracing::info!(
        user_id = user.id,
        slcb_username = %slcb.username,
        hours = slcb.hours,
        points = slcb.points,
        force,
        "force-linked SLCB data to user"
    );

    Ok(ForcedLinkSummary {
        user_id: user.id,
        slcb_username: slcb.username,
        hours_credited: slcb.hours,
        points_credited: slcb.points,
        watched_time_after: new_watched,
        boonbucks_after: new_boonbucks,
    })
}

/// Walk all linked YouTube channels and import any matching SLCB row that
/// hasn't already been imported.
pub async fn match_youtube_links(
    db: &AlwaysCloneableConnection,
) -> Result<MatchSummary, SlcbError> {
    let mut summary = MatchSummary::default();

    let links = entities::connected_youtube_accounts::Entity::find()
        .all(&**db)
        .await?;

    for link in links {
        summary.considered += 1;

        let user = entities::users::Entity::find_by_id(link.user_id)
            .one(&**db)
            .await?;
        let Some(user) = user else {
            summary.no_slcb_row += 1;
            continue;
        };
        if user.migrated {
            summary.already_migrated += 1;
            continue;
        }

        let slcb = entities::slcb_currency::Entity::find()
            .filter(entities::slcb_currency::Column::UserId.eq(link.youtube_channel_id.as_str()))
            .one(&**db)
            .await?;
        let Some(slcb) = slcb else {
            summary.no_slcb_row += 1;
            continue;
        };

        let new_watched = user.watched_time + (slcb.hours as i64) * 3600;
        let new_boonbucks = user.boonbucks + slcb.points;
        entities::users::Entity::update(entities::users::ActiveModel {
            id: ActiveValue::unchanged(user.id),
            watched_time: Set(new_watched),
            boonbucks: Set(new_boonbucks),
            migrated: Set(true),
            ..Default::default()
        })
        .exec(&**db)
        .await?;

        summary.matched += 1;
        tracing::info!(
            user_id = user.id,
            channel = %link.youtube_channel_id,
            slcb_username = %slcb.username,
            hours = slcb.hours,
            points = slcb.points,
            "imported SLCB data for user"
        );
    }

    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pascal_case_streamlabs_export() {
        let json = r#"[
            {"Name":"alice","UserID":"UCabc","Hours":10,"Points":100},
            {"Name":"bob","UserID":null,"Hours":5,"Points":50}
        ]"#;
        let records: Vec<StreamlabsRecord> = serde_json::from_str(json).unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].username, "alice");
        assert_eq!(records[0].user_id.as_deref(), Some("UCabc"));
        assert_eq!(records[0].hours, 10);
        assert_eq!(records[0].points, 100);
        assert!(records[1].user_id.is_none());
    }

    #[test]
    fn parses_snake_case_export() {
        let json = r#"[{"username":"carol","user_id":"UCcarol","hours":7,"points":33}]"#;
        let records: Vec<StreamlabsRecord> = serde_json::from_str(json).unwrap();
        assert_eq!(records[0].username, "carol");
        assert_eq!(records[0].user_id.as_deref(), Some("UCcarol"));
    }

    #[test]
    fn missing_optional_fields_default_to_zero() {
        let json = r#"[{"Name":"dave"}]"#;
        let records: Vec<StreamlabsRecord> = serde_json::from_str(json).unwrap();
        assert_eq!(records[0].username, "dave");
        assert_eq!(records[0].hours, 0);
        assert_eq!(records[0].points, 0);
        assert!(records[0].user_id.is_none());
    }
}
