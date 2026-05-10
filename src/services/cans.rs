use std::{fmt::Display, sync::Arc};

use chrono::Utc;
use reqwest::StatusCode;

use crate::{
    ServiceRegistry,
    dtos::error::{PublicError, ToPublicError},
    entities,
    repositories::{
        AlwaysCloneableConnection, BaseRepository, RepositoryError,
        cans::{CanRepositoryExt, CreateCanDto},
    },
    services::cooldowns::{
        CooldownService, CooldownServiceError, GlobalCooldown, global::CanCooldown,
    },
};

use super::UserId;

#[derive(Debug)]
pub enum CanType {
    Can,
    Bear,
}

impl Display for CanType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CanType::Can => write!(f, "Can"),
            CanType::Bear => write!(f, "Bear"),
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum CansServiceError {
    #[error("{0} cooldown active.")]
    OnCooldown(CanType, i64, u64),
    #[error("Error while checking cooldown")]
    Cooldown(#[from] CooldownServiceError),
    #[error(transparent)]
    Repository(#[from] RepositoryError),
}

fn can_name(prefix: &str, number_of_cans: u64) -> String {
    match number_of_cans {
        (0..=49_999) => format!("{prefix} Town"),
        (50_000..=999_999) => format!("{prefix} City"),
        (1_000_000..=49_999_999) => format!("{prefix} Country"),
        (50_000_000..=99_999_999) => format!("{prefix} Continent"),
        (100_000_000..=999_999_999) => format!("{prefix} Planet"),
        (1_000_000_000..=4_999_999_999) => format!("{prefix} Galaxy"),
        (5_000_000_000..=9_999_999_999) => format!("{prefix} Universe"),
        _ => format!("{prefix}finity"),
    }
}

fn cooldown_errors(can_type: CanType, current_count: u64, seconds_remaining: i64) -> String {
    let prefix = can_type.to_string();
    let item = prefix.to_lowercase();
    let structure = can_name(&prefix, current_count);

    match current_count {
        0..=49_999 => {
            // Town tier - small bureaucracy
            let messages = [
                format!(
                    "{structure}'s planning committee needs {seconds_remaining}s to review your application."
                ),
                format!(
                    "The Mayor of {structure} requires a {seconds_remaining} second waiting period between {item}s."
                ),
                format!(
                    "{structure}'s building inspector is still processing your last {item}. {seconds_remaining}s remaining."
                ),
                format!(
                    "Whoa there! {structure}'s zoning laws prohibit rapid {item} placement. {seconds_remaining}s cooldown."
                ),
            ];
            messages[rand::random::<u64>() as usize % messages.len()].clone()
        }
        50_000..=999_999 => {
            // City tier - urban bureaucracy
            let messages = [
                format!(
                    "{structure}'s zoning board is still reviewing your last permit. {seconds_remaining}s remaining."
                ),
                format!(
                    "The Department of {prefix} Affairs is processing paperwork. Please wait {seconds_remaining} seconds."
                ),
                format!(
                    "{structure}'s construction crews are on union break. {seconds_remaining}s until they return."
                ),
                format!(
                    "Your {item} is stuck in {structure} customs. Estimated processing: {seconds_remaining} seconds."
                ),
                format!(
                    "{structure}'s infrastructure is at capacity. Expansion ETA: {seconds_remaining} seconds."
                ),
            ];
            messages[rand::random::<u64>() as usize % messages.len()].clone()
        }
        1_000_000..=49_999_999 => {
            // Country tier - national bureaucracy
            let messages = [
                format!(
                    "{structure}'s immigration office is backed up! Border processing: {seconds_remaining}s."
                ),
                format!(
                    "National {prefix} Registry requires {seconds_remaining}s between additions for census purposes."
                ),
                format!(
                    "{structure}'s Department of Internal {prefix}s is reviewing your paperwork. {seconds_remaining}s remaining."
                ),
                format!(
                    "Your {item} visa is being processed. Estimated wait: {seconds_remaining} seconds."
                ),
            ];
            messages[rand::random::<u64>() as usize % messages.len()].clone()
        }
        50_000_000..=99_999_999 => {
            // Continent tier - continental bureaucracy
            let messages = [
                format!(
                    "{structure}'s continental parliament is in session. {seconds_remaining}s until next {item} can be ratified."
                ),
                format!(
                    "Inter-continental {prefix} treaties require a {seconds_remaining}s cooling period."
                ),
                format!(
                    "{structure}'s geological survey team needs {seconds_remaining}s to assess stability."
                ),
            ];
            messages[rand::random::<u64>() as usize % messages.len()].clone()
        }
        100_000_000..=999_999_999 => {
            // Planet tier - planetary bureaucracy
            let messages = [
                format!(
                    "{structure}'s orbital defense system is recalibrating. Next {item} cleared in {seconds_remaining}s."
                ),
                format!(
                    "Planetary {prefix} Council requires {seconds_remaining}s between atmospheric entries."
                ),
                format!(
                    "{structure}'s gravity well is still settling from your last {item}. {seconds_remaining}s remaining."
                ),
                format!(
                    "Your {item} is in orbital quarantine. Clearance in {seconds_remaining} seconds."
                ),
            ];
            messages[rand::random::<u64>() as usize % messages.len()].clone()
        }
        1_000_000_000..=4_999_999_999 => {
            // Galaxy tier - galactic bureaucracy
            let messages = [
                format!(
                    "{structure}'s intergalactic treaty prohibits faster-than-{item} travel. Warp cooldown: {seconds_remaining}s."
                ),
                format!(
                    "The Galactic {prefix} Federation requires {seconds_remaining}s between shipments."
                ),
                format!(
                    "Your {item} is traveling at sub-light speed. ETA to {structure}: {seconds_remaining}s."
                ),
                format!(
                    "{structure}'s hyperspace lanes are congested. Next jump window: {seconds_remaining}s."
                ),
            ];
            messages[rand::random::<u64>() as usize % messages.len()].clone()
        }
        5_000_000_000..=9_999_999_999 => {
            // Universe tier - universal bureaucracy
            let messages = [
                format!(
                    "{structure} is still expanding from your last contribution. Cosmic cooldown: {seconds_remaining}s."
                ),
                format!(
                    "The fabric of {structure} needs {seconds_remaining}s to stabilize before accepting another {item}."
                ),
                format!(
                    "Universal {prefix} constant is recalculating. Please wait {seconds_remaining} seconds."
                ),
                format!(
                    "Your {item} is traversing the cosmic microwave background. {seconds_remaining}s remaining."
                ),
            ];
            messages[rand::random::<u64>() as usize % messages.len()].clone()
        }
        _ => {
            // Canfinity tier - transcendent
            let messages = [
                format!(
                    "{structure} has transcended the concept of time, but you still need to wait {seconds_remaining} seconds."
                ),
                format!(
                    "{structure} exists beyond comprehension, yet bureaucracy persists. {seconds_remaining}s cooldown."
                ),
                format!(
                    "Your {item} is everywhere and nowhere. Processing time: {seconds_remaining}s."
                ),
                format!(
                    "{structure}'s infinite nature requires a finite {seconds_remaining}s waiting period. Paradox accepted."
                ),
            ];
            messages[rand::random::<u64>() as usize % messages.len()].clone()
        }
    }
}

impl ToPublicError for CansServiceError {
    fn as_public(&self) -> Option<crate::dtos::error::PublicError> {
        match self {
            CansServiceError::OnCooldown(can_type, seconds, can_count) => match can_type {
                CanType::Can => Some(PublicError::with_owned(
                    "can-cooldown",
                    cooldown_errors(CanType::Can, *can_count, *seconds),
                    StatusCode::TOO_MANY_REQUESTS,
                )),
                CanType::Bear => Some(PublicError::with_owned(
                    "bear-cooldown",
                    cooldown_errors(CanType::Bear, *can_count, *seconds),
                    StatusCode::TOO_MANY_REQUESTS,
                )),
            },
            CansServiceError::Cooldown(e) => e.as_public(),
            _ => None,
        }
    }
}

pub struct CansService {
    can_repo: BaseRepository<entities::cans::Entity>,
    cooldown_service: Arc<CooldownService>,
}

impl CansService {
    pub fn new(db: &AlwaysCloneableConnection, registry: &ServiceRegistry) -> Self {
        Self {
            can_repo: BaseRepository::new(db),
            cooldown_service: registry.cooldown_service(),
        }
    }

    pub async fn count(&self) -> Result<u64, CansServiceError> {
        let count = self.can_repo.count().await?;

        Ok(count)
    }

    pub async fn add(&self, user_id: UserId, can_type: CanType) -> Result<(), CansServiceError> {
        if let Some(expires_at) = CanCooldown.get(&self.cooldown_service).await? {
            let now = Utc::now().naive_utc();
            let expires_secs = expires_at.signed_duration_since(now).num_seconds();
            return Err(CansServiceError::OnCooldown(
                can_type,
                expires_secs,
                self.count().await?,
            ));
        }

        self.can_repo
            .add(CreateCanDto {
                added_by: user_id.into(),
                legit: true,
            })
            .await?;

        CanCooldown.set(&self.cooldown_service).await?;

        Ok(())
    }
}
