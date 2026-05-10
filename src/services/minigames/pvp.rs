//! PvP minigame: 50/50 duel between two users for a fixed bet.
//!
//! Ported from `byers` (`pvp.rs`). The Discord-only accept/decline interactive
//! flow is intentionally dropped — this service resolves the duel atomically
//! the moment the API is called. Calliope and the (future) bot are responsible
//! for any UI confirmation step before invoking this endpoint.
//!
//! Bot-as-opponent mode (90/10 against challenger, jackpot funnel) is **not**
//! implemented yet; it requires a configured bot user id which Caliborn
//! does not yet have. Tracked as a follow-up for config wiring.

use std::sync::Arc;

use rand::Rng;
use reqwest::StatusCode;
use sea_orm::Set;
use serde::Serialize;
use shared_constants::permissions::PERM_USE_MINIGAMES;
use utoipa::ToSchema;

use crate::{
    RepositoryError,
    dtos::error::{PublicError, ToPublicError},
    entities,
    repositories::{
        BaseRepository,
        users::{TransferError, UserRepositoryExt},
    },
    services::{
        UserId,
        cooldowns::{CooldownService, CooldownServiceError, UserCooldown, user::PvPCooldown},
        users::{UserService, UserServiceError},
    },
};

const PVP_GAME_NAME: &str = "pvp";
const PVP_BET: i32 = 10;

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PvpResult {
    pub challenger_id: i64,
    pub opponent_id: i64,
    pub challenger_won: bool,
    pub transferred: i32,
    pub challenger_balance: i32,
    pub opponent_balance: i32,
}

#[derive(thiserror::Error, Debug)]
pub enum PvpServiceError {
    #[error("Cannot challenge yourself")]
    SelfChallenge,
    #[error("Challenger is on cooldown")]
    OnCooldown,
    #[error("Challenger has insufficient funds")]
    ChallengerInsufficientFunds,
    #[error("Opponent has insufficient funds")]
    OpponentInsufficientFunds,
    #[error("Opponent not found")]
    OpponentNotFound,

    #[error(transparent)]
    Cooldown(#[from] CooldownServiceError),
    #[error(transparent)]
    UserService(#[from] UserServiceError),
    #[error(transparent)]
    Repository(#[from] RepositoryError),
    #[error(transparent)]
    Db(#[from] sea_orm::DbErr),
}

impl ToPublicError for PvpServiceError {
    fn as_public(&self) -> Option<PublicError> {
        match self {
            PvpServiceError::SelfChallenge => Some(PublicError::new(
                "self-challenge",
                "You cannot challenge yourself.",
                StatusCode::UNPROCESSABLE_ENTITY,
            )),
            PvpServiceError::OnCooldown => Some(PublicError::new(
                "on-cooldown",
                "You need to rest before challenging again.",
                StatusCode::TOO_MANY_REQUESTS,
            )),
            PvpServiceError::ChallengerInsufficientFunds => Some(PublicError::new(
                "challenger-insufficient-funds",
                "You don't have enough boonbucks to challenge.",
                StatusCode::UNPROCESSABLE_ENTITY,
            )),
            PvpServiceError::OpponentInsufficientFunds => Some(PublicError::new(
                "opponent-insufficient-funds",
                "Your opponent does not have enough boonbucks.",
                StatusCode::UNPROCESSABLE_ENTITY,
            )),
            PvpServiceError::OpponentNotFound => Some(PublicError::new(
                "opponent-not-found",
                "Opponent user not found.",
                StatusCode::UNPROCESSABLE_ENTITY,
            )),
            PvpServiceError::UserService(e) => e.as_public(),
            _ => None,
        }
    }
}

pub struct PvpService {
    user_repo: BaseRepository<entities::users::Entity>,
    history_repo: BaseRepository<entities::minigame_history::Entity>,
    cooldown_service: Arc<CooldownService>,
    user_service: Arc<UserService>,
}

impl PvpService {
    pub fn new(
        user_repo: BaseRepository<entities::users::Entity>,
        history_repo: BaseRepository<entities::minigame_history::Entity>,
        cooldown_service: Arc<CooldownService>,
        user_service: Arc<UserService>,
    ) -> Self {
        Self {
            user_repo,
            history_repo,
            cooldown_service,
            user_service,
        }
    }

    /// Challenge `opponent` to a duel for [`PVP_BET`] boonbucks.
    ///
    /// Resolves immediately with a 50/50 RNG outcome. The losing side's balance
    /// is debited by [`PVP_BET`] atomically; the winning side is credited the
    /// same amount. Both cooldowns are set on success.
    pub async fn challenge(
        &self,
        challenger: UserId,
        opponent: UserId,
    ) -> Result<PvpResult, PvpServiceError> {
        let challenger_id: i64 = challenger.into();
        let opponent_id: i64 = opponent.into();
        if challenger_id == opponent_id {
            return Err(PvpServiceError::SelfChallenge);
        }

        self.user_service
            .user_has_permission(challenger, PERM_USE_MINIGAMES)
            .await?;

        let cooldown = PvPCooldown;
        if cooldown
            .on_cooldown(&self.cooldown_service, challenger)
            .await?
        {
            return Err(PvpServiceError::OnCooldown);
        }

        // Auto-create both rows (matches byers `Users::get_or_insert`).
        let challenger_user = self.user_service.get_user(challenger).await?;
        let opponent_user = self.user_service.get_user(opponent).await?;

        if challenger_user.boonbucks < PVP_BET {
            return Err(PvpServiceError::ChallengerInsufficientFunds);
        }
        if opponent_user.boonbucks < PVP_BET {
            return Err(PvpServiceError::OpponentInsufficientFunds);
        }

        let challenger_won = {
            let mut rng = rand::rng();
            rng.random_bool(0.5)
        };

        let (loser, winner) = if challenger_won {
            (opponent_id, challenger_id)
        } else {
            (challenger_id, opponent_id)
        };

        let (loser_balance, winner_balance) = match self
            .user_repo
            .transfer_boonbucks(loser, winner, PVP_BET)
            .await
        {
            Ok(b) => b,
            Err(TransferError::InsufficientFunds) => {
                // Race between pre-check and transfer. Map to the side that ran out.
                return Err(if challenger_won {
                    PvpServiceError::OpponentInsufficientFunds
                } else {
                    PvpServiceError::ChallengerInsufficientFunds
                });
            }
            Err(TransferError::SenderNotFound) | Err(TransferError::RecipientNotFound) => {
                return Err(PvpServiceError::OpponentNotFound);
            }
            Err(TransferError::Db(e)) => return Err(PvpServiceError::Db(e)),
        };

        let (challenger_balance, opponent_balance) = if challenger_won {
            (winner_balance, loser_balance)
        } else {
            (loser_balance, winner_balance)
        };

        cooldown
            .set_or_replace(&self.cooldown_service, challenger)
            .await?;
        cooldown
            .set_or_replace(&self.cooldown_service, opponent)
            .await?;

        let result_json = serde_json::json!({
            "challenger_id": challenger_id,
            "opponent_id": opponent_id,
            "challenger_won": challenger_won,
            "transferred": PVP_BET,
        });
        let payout_for_challenger = if challenger_won { PVP_BET } else { 0 };
        let wager_for_challenger = if challenger_won { 0 } else { PVP_BET };
        self.history_repo
            .add(entities::minigame_history::ActiveModel {
                user_id: Set(challenger_id),
                game: Set(PVP_GAME_NAME.to_string()),
                wager: Set(wager_for_challenger),
                payout: Set(payout_for_challenger),
                result: Set(Some(result_json)),
                ..Default::default()
            })
            .await?;

        Ok(PvpResult {
            challenger_id,
            opponent_id,
            challenger_won,
            transferred: PVP_BET,
            challenger_balance,
            opponent_balance,
        })
    }
}
