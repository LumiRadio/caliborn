//! Dice-roll logic and service. Ported from `byers` (`roll_dice.rs`).

use std::sync::Arc;

use rand::Rng;
use reqwest::StatusCode;
use sea_orm::{ActiveValue, EntityTrait, Set};
use serde::Serialize;
use shared_constants::permissions::PERM_USE_MINIGAMES;
use utoipa::ToSchema;

use crate::{
    RepositoryError,
    dtos::error::{PublicError, ToPublicError},
    entities,
    repositories::{
        AlwaysCloneableConnection, BaseRepository,
        users::{BalanceUpdateError, UserRepositoryExt},
    },
    services::{
        UserId,
        cooldowns::{CooldownService, CooldownServiceError, UserCooldown, user::RollDiceCooldown},
        users::{UserService, UserServiceError},
    },
};

const DICE_GAME_NAME: &str = "dice";
const DICE_BET: i32 = 5;
const SECRET_BONUS_3D: i32 = 75;
const SECRET_BONUS_4D: i32 = 100;

/// Pure scoring/joining logic for a dice roll.
#[derive(Debug, Clone)]
pub struct DiceOutcome {
    /// Mode in effect for this roll (3 or 4 dice).
    pub mode: u8,
    /// All four rolled dice values (only first 3 used in mode-3).
    pub dice: [u8; 4],
    /// The "stitched" player roll value (e.g. 463 in 3-dice, 4632 in 4-dice).
    pub player_roll: i32,
    /// Sum of dice in play.
    pub sum: u32,
    /// Pre-bonus winnings (already includes the ×5 multiplier from byers).
    pub base_winnings: i32,
    /// True when `player_roll == server_roll`.
    pub secret_match: bool,
    /// Total winnings credited to the player on success (`base + bonus`).
    pub total_winnings: i32,
}

/// Roll three or four d6, score against the current `server_roll`.
///
/// `dice` must be values in `1..=6`. Caller is responsible for generating them.
pub fn play(dice: [u8; 4], server_roll: i32) -> DiceOutcome {
    if server_roll < 1000 {
        let player_roll = dice[0] as i32 * 100 + dice[1] as i32 * 10 + dice[2] as i32;
        let sum = (dice[0] + dice[1] + dice[2]) as u32;
        let base_winnings = match sum {
            0..=10 => 0,
            11..=14 => 2,
            15 => 3,
            16 => 4,
            17 => 5,
            18 => 10,
            _ => unreachable!("invalid 3d6 sum {sum}"),
        } * 5;
        let secret_match = player_roll == server_roll;
        let bonus = if secret_match { SECRET_BONUS_3D } else { 0 };
        DiceOutcome {
            mode: 3,
            dice,
            player_roll,
            sum,
            base_winnings,
            secret_match,
            total_winnings: base_winnings + bonus,
        }
    } else {
        let player_roll =
            dice[0] as i32 * 1000 + dice[1] as i32 * 100 + dice[2] as i32 * 10 + dice[3] as i32;
        let sum = (dice[0] + dice[1] + dice[2] + dice[3]) as u32;
        let base_winnings = match sum {
            0..=13 => 0,
            14..=18 => 2,
            19..=21 => 3,
            22 => 5,
            23 => 7,
            24 => 15,
            _ => unreachable!("invalid 4d6 sum {sum}"),
        } * 5;
        let secret_match = player_roll == server_roll;
        let bonus = if secret_match { SECRET_BONUS_4D } else { 0 };
        DiceOutcome {
            mode: 4,
            dice,
            player_roll,
            sum,
            base_winnings,
            secret_match,
            total_winnings: base_winnings + bonus,
        }
    }
}

/// Advance the server's quest roll. Mirrors the byers `roll_over` exactly.
pub fn roll_over(mut roll: i32) -> i32 {
    if roll == 666 {
        return 1111;
    }

    if roll < 1000 {
        let hundreds = roll / 100;
        let tens = (roll % 100) / 10;
        let ones = roll % 10;
        if ones == 6 {
            if tens == 6 {
                roll = (hundreds + 1) * 100 + 11;
            } else {
                roll = hundreds * 100 + (tens + 1) * 10 + 1;
            }
        } else {
            roll += 1;
        }
    } else {
        if roll == 6666 {
            return 1111;
        }
        let thousands = roll / 1000;
        let hundreds = (roll % 1000) / 100;
        let tens = (roll % 100) / 10;
        let ones = roll % 10;
        if ones == 6 {
            if tens == 6 {
                if hundreds == 6 {
                    roll = (thousands + 1) * 1000 + 111;
                } else {
                    roll = thousands * 1000 + (hundreds + 1) * 100 + 11;
                }
            } else {
                roll = thousands * 1000 + hundreds * 100 + (tens + 1) * 10 + 1;
            }
        } else {
            roll += 1;
        }
    }
    roll
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct RollResult {
    pub mode: u8,
    pub dice: Vec<u8>,
    pub player_roll: i32,
    pub sum: u32,
    pub server_roll_before: i32,
    pub server_roll_after: i32,
    pub server_mode_after: u8,
    pub secret_match: bool,
    pub bet: i32,
    pub payout: i32,
    pub new_balance: i32,
    pub won: bool,
}

#[derive(thiserror::Error, Debug)]
pub enum DiceServiceError {
    #[error("On cooldown")]
    OnCooldown,
    #[error("Insufficient funds")]
    InsufficientFunds,
    #[error("User does not exist")]
    UserNotFound,
    #[error("Radio state singleton missing")]
    RadioStateMissing,

    #[error(transparent)]
    Cooldown(#[from] CooldownServiceError),
    #[error(transparent)]
    UserService(#[from] UserServiceError),
    #[error(transparent)]
    Repository(#[from] RepositoryError),
    #[error(transparent)]
    Db(#[from] sea_orm::DbErr),
}

impl From<BalanceUpdateError> for DiceServiceError {
    fn from(value: BalanceUpdateError) -> Self {
        match value {
            BalanceUpdateError::UserNotFound => Self::UserNotFound,
            BalanceUpdateError::InsufficientFunds => Self::InsufficientFunds,
            BalanceUpdateError::Db(e) => Self::Db(e),
        }
    }
}

impl ToPublicError for DiceServiceError {
    fn as_public(&self) -> Option<PublicError> {
        match self {
            DiceServiceError::OnCooldown => Some(PublicError::new(
                "on-cooldown",
                "Dice are still being polished.",
                StatusCode::TOO_MANY_REQUESTS,
            )),
            DiceServiceError::InsufficientFunds => Some(PublicError::new(
                "insufficient-funds",
                "You need at least 5 boonbucks to roll the dice.",
                StatusCode::UNPROCESSABLE_ENTITY,
            )),
            DiceServiceError::UserNotFound => Some(PublicError::new(
                "user-not-found",
                "User not found.",
                StatusCode::UNPROCESSABLE_ENTITY,
            )),
            DiceServiceError::UserService(e) => e.as_public(),
            _ => None,
        }
    }
}

pub struct DiceService {
    db: AlwaysCloneableConnection,
    user_repo: BaseRepository<entities::users::Entity>,
    history_repo: BaseRepository<entities::minigame_history::Entity>,
    cooldown_service: Arc<CooldownService>,
    user_service: Arc<UserService>,
}

impl DiceService {
    pub fn new(
        db: &AlwaysCloneableConnection,
        cooldown_service: Arc<CooldownService>,
        user_service: Arc<UserService>,
    ) -> Self {
        Self {
            db: db.clone(),
            user_repo: BaseRepository::new(db),
            history_repo: BaseRepository::new(db),
            cooldown_service,
            user_service,
        }
    }

    /// Roll the dice for `user_id` against the current radio quest roll.
    pub async fn roll(&self, user_id: UserId) -> Result<RollResult, DiceServiceError> {
        self.user_service
            .user_has_permission(user_id, PERM_USE_MINIGAMES)
            .await?;

        let cooldown = RollDiceCooldown;
        if cooldown
            .on_cooldown(&self.cooldown_service, user_id)
            .await?
        {
            return Err(DiceServiceError::OnCooldown);
        }

        let radio = entities::radio_state::Entity::find_by_id(1_i16)
            .one(&*self.db)
            .await?
            .ok_or(DiceServiceError::RadioStateMissing)?;
        let server_roll_before = radio.dice_roll_target;

        let dice: [u8; 4] = {
            let mut rng = rand::rng();
            [
                rng.random_range(1..=6),
                rng.random_range(1..=6),
                rng.random_range(1..=6),
                rng.random_range(1..=6),
            ]
        };
        let outcome = play(dice, server_roll_before);

        let new_balance = self
            .user_repo
            .apply_minigame_outcome(user_id.into(), DICE_BET, outcome.total_winnings)
            .await?;

        let (server_roll_after, server_mode_after) = if outcome.secret_match {
            let next = roll_over(server_roll_before);
            let mode = if next < 1000 { 3_i16 } else { 4_i16 };
            entities::radio_state::Entity::update(entities::radio_state::ActiveModel {
                id: ActiveValue::unchanged(1),
                dice_roll_target: Set(next),
                dice_roll_mode: Set(mode),
                updated_at: Set(chrono::Utc::now().naive_utc()),
                ..Default::default()
            })
            .exec(&*self.db)
            .await?;
            (next, mode as u8)
        } else {
            (server_roll_before, outcome.mode)
        };

        cooldown
            .set_or_replace(&self.cooldown_service, user_id)
            .await?;

        let dice_played = match outcome.mode {
            3 => outcome.dice[..3].to_vec(),
            _ => outcome.dice.to_vec(),
        };
        let result_json = serde_json::json!({
            "mode": outcome.mode,
            "dice": dice_played,
            "player_roll": outcome.player_roll,
            "sum": outcome.sum,
            "server_roll_before": server_roll_before,
            "server_roll_after": server_roll_after,
            "secret_match": outcome.secret_match,
        });
        self.history_repo
            .add(entities::minigame_history::ActiveModel {
                user_id: Set(user_id.into()),
                game: Set(DICE_GAME_NAME.to_string()),
                wager: Set(DICE_BET),
                payout: Set(outcome.total_winnings),
                result: Set(Some(result_json)),
                ..Default::default()
            })
            .await?;

        Ok(RollResult {
            mode: outcome.mode,
            dice: dice_played,
            player_roll: outcome.player_roll,
            sum: outcome.sum,
            server_roll_before,
            server_roll_after,
            server_mode_after,
            secret_match: outcome.secret_match,
            bet: DICE_BET,
            payout: outcome.total_winnings,
            new_balance,
            won: outcome.total_winnings > 0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rollover_3d_basic() {
        assert_eq!(roll_over(111), 112);
        assert_eq!(roll_over(116), 121);
        assert_eq!(roll_over(126), 131);
        assert_eq!(roll_over(136), 141);
        assert_eq!(roll_over(146), 151);
        assert_eq!(roll_over(156), 161);
        assert_eq!(roll_over(166), 211);
    }

    #[test]
    fn rollover_3d_to_4d_transition() {
        assert_eq!(roll_over(666), 1111);
    }

    #[test]
    fn rollover_4d_basic() {
        assert_eq!(roll_over(1111), 1112);
        assert_eq!(roll_over(1116), 1121);
        assert_eq!(roll_over(1126), 1131);
        assert_eq!(roll_over(1166), 1211);
        assert_eq!(roll_over(1266), 1311);
        assert_eq!(roll_over(1666), 2111);
    }

    #[test]
    fn rollover_4d_resets_at_6666() {
        assert_eq!(roll_over(6666), 1111);
    }

    #[test]
    fn play_3d_loses_on_low_sum() {
        // sum = 5, no match
        let o = play([1, 2, 2, 0], 111);
        assert_eq!(o.mode, 3);
        assert_eq!(o.sum, 5);
        assert_eq!(o.base_winnings, 0);
        assert!(!o.secret_match);
        assert_eq!(o.total_winnings, 0);
    }

    #[test]
    fn play_3d_pays_2_at_sum_11_to_14() {
        let o = play([4, 4, 3, 0], 111);
        assert_eq!(o.sum, 11);
        assert_eq!(o.base_winnings, 10); // 2 * 5
        assert_eq!(o.total_winnings, 10);
    }

    #[test]
    fn play_3d_pays_10_at_sum_18() {
        let o = play([6, 6, 6, 0], 111);
        assert_eq!(o.sum, 18);
        assert_eq!(o.base_winnings, 50); // 10 * 5
    }

    #[test]
    fn play_3d_secret_adds_75() {
        let o = play([4, 6, 3, 0], 463);
        assert!(o.secret_match);
        // sum 13 → tier 2 → 10
        assert_eq!(o.base_winnings, 10);
        assert_eq!(o.total_winnings, 10 + 75);
    }

    #[test]
    fn play_3d_secret_only_when_base_zero() {
        // sum 4 → 0 base. roll = 121. secret match → +75.
        let o = play([1, 2, 1, 0], 121);
        assert!(o.secret_match);
        assert_eq!(o.base_winnings, 0);
        assert_eq!(o.total_winnings, 75);
    }

    #[test]
    fn play_4d_pays_15_at_sum_24() {
        let o = play([6, 6, 6, 6], 1111);
        assert_eq!(o.mode, 4);
        assert_eq!(o.sum, 24);
        assert_eq!(o.base_winnings, 75); // 15 * 5
        assert!(!o.secret_match);
    }

    #[test]
    fn play_4d_secret_adds_100() {
        let o = play([1, 2, 3, 4], 1234);
        assert!(o.secret_match);
        // sum 10 → 0
        assert_eq!(o.base_winnings, 0);
        assert_eq!(o.total_winnings, 100);
    }
}
