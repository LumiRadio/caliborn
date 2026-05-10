//! Slot-machine logic and service. Ported from `byers` (`new_slots.rs`).

use std::sync::Arc;

use rand::{Rng, seq::IndexedRandom};
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
        users::{BalanceUpdateError, UserRepositoryExt},
    },
    services::{
        UserId,
        cooldowns::{CooldownService, CooldownServiceError, UserCooldown, user::SlotCooldown},
        users::{UserService, UserServiceError},
    },
};

const SLOTS_GAME_NAME: &str = "slots";
const MIN_BET: i32 = 1;
const MAX_BET: i32 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SlotSymbol {
    Jackpot,
    RedSeven,
    TripleBar,
    DoubleBar,
    Bar,
    Cherry,
}

impl SlotSymbol {
    pub const ALL: [SlotSymbol; 6] = [
        SlotSymbol::Jackpot,
        SlotSymbol::RedSeven,
        SlotSymbol::TripleBar,
        SlotSymbol::DoubleBar,
        SlotSymbol::Bar,
        SlotSymbol::Cherry,
    ];

    pub fn weight(self) -> u32 {
        match self {
            Self::Jackpot => 6,
            Self::RedSeven => 8,
            Self::TripleBar => 9,
            Self::DoubleBar => 11,
            Self::Bar => 22,
            Self::Cherry => 8,
        }
    }

    pub fn is_bar(self) -> bool {
        matches!(self, Self::Bar | Self::DoubleBar | Self::TripleBar)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ReelSymbol {
    Symbol { symbol: SlotSymbol },
    Blank,
}

impl ReelSymbol {
    fn is_bar(self) -> bool {
        match self {
            Self::Symbol { symbol } => symbol.is_bar(),
            Self::Blank => false,
        }
    }
}

impl PartialEq<SlotSymbol> for ReelSymbol {
    fn eq(&self, other: &SlotSymbol) -> bool {
        match self {
            Self::Symbol { symbol } => symbol == other,
            Self::Blank => false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Reel {
    pub symbols: Vec<ReelSymbol>,
}

impl Reel {
    pub fn new() -> Self {
        let mut symbol_reel: Vec<ReelSymbol> = Vec::with_capacity(64);
        let total_symbol_count: u32 = SlotSymbol::ALL.iter().map(|s| s.weight()).sum();

        for &symbol in SlotSymbol::ALL.iter() {
            let count = symbol_reel
                .iter()
                .filter(|s| match s {
                    ReelSymbol::Symbol { symbol: sy } => *sy == symbol,
                    ReelSymbol::Blank => false,
                })
                .count();
            let weight = symbol.weight();
            if (count as u32) < weight
                && (count as f32)
                    <= (weight as f32) / (total_symbol_count as f32) * (symbol_reel.len() as f32)
            {
                symbol_reel.push(ReelSymbol::Symbol { symbol });
            }
        }

        let mut reel = Vec::with_capacity(symbol_reel.len() * 2);
        for s in symbol_reel {
            reel.push(s);
            reel.push(ReelSymbol::Blank);
        }
        Self { symbols: reel }
    }

    pub fn spin<R: Rng + ?Sized>(&self, rng: &mut R) -> ReelSymbol {
        *self.symbols.choose(rng).expect("reel must not be empty")
    }
}

impl Default for Reel {
    fn default() -> Self {
        Self::new()
    }
}

pub struct SlotMachine {
    pub reels: [Reel; 3],
}

impl SlotMachine {
    pub fn new() -> Self {
        let reel = Reel::new();
        Self {
            reels: [reel.clone(), reel.clone(), reel],
        }
    }

    /// Spin three reels and return `(payout_multiplier, reels)`.
    /// `payout_multiplier == 0` means a loss.
    pub fn spin<R: Rng + ?Sized>(&self, rng: &mut R) -> (u32, [ReelSymbol; 3]) {
        let reel = [
            self.reels[0].spin(rng),
            self.reels[1].spin(rng),
            self.reels[2].spin(rng),
        ];
        let multiplier = score(&reel);
        (multiplier, reel)
    }
}

impl Default for SlotMachine {
    fn default() -> Self {
        Self::new()
    }
}

/// Pure scoring function from a fixed reel triple.
pub fn score(reel: &[ReelSymbol; 3]) -> u32 {
    use SlotSymbol::*;

    let count = |target: SlotSymbol| reel.iter().filter(|s| **s == target).count();

    if count(Jackpot) == 3 {
        return 1200;
    }
    if count(RedSeven) == 3 {
        return 200;
    }
    if count(TripleBar) == 3 {
        return 100;
    }
    if count(DoubleBar) == 3 {
        return 90;
    }
    if count(Bar) == 3 || count(Cherry) == 3 {
        return 40;
    }
    if reel.iter().all(|s| s.is_bar()) {
        return 10;
    }
    if count(Cherry) == 2 {
        return 5;
    }
    if count(Cherry) == 1 {
        return 1;
    }
    0
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SpinResult {
    pub reels: [ReelSymbol; 3],
    pub multiplier: u32,
    pub payout: i32,
    pub bet: i32,
    pub new_balance: i32,
    pub won: bool,
}

#[derive(thiserror::Error, Debug)]
pub enum SlotsServiceError {
    #[error("Bet must be between {MIN_BET} and {MAX_BET}")]
    BetOutOfRange,
    #[error("On cooldown")]
    OnCooldown,
    #[error("Insufficient funds")]
    InsufficientFunds,
    #[error("User does not exist")]
    UserNotFound,

    #[error(transparent)]
    Cooldown(#[from] CooldownServiceError),
    #[error(transparent)]
    UserService(#[from] UserServiceError),
    #[error(transparent)]
    Repository(#[from] RepositoryError),
    #[error(transparent)]
    Db(#[from] sea_orm::DbErr),
}

impl From<BalanceUpdateError> for SlotsServiceError {
    fn from(value: BalanceUpdateError) -> Self {
        match value {
            BalanceUpdateError::UserNotFound => Self::UserNotFound,
            BalanceUpdateError::InsufficientFunds => Self::InsufficientFunds,
            BalanceUpdateError::Db(e) => Self::Db(e),
        }
    }
}

impl ToPublicError for SlotsServiceError {
    fn as_public(&self) -> Option<PublicError> {
        match self {
            SlotsServiceError::BetOutOfRange => Some(PublicError::with_owned(
                "bet-out-of-range",
                self.to_string(),
                StatusCode::UNPROCESSABLE_ENTITY,
            )),
            SlotsServiceError::OnCooldown => Some(PublicError::new(
                "on-cooldown",
                "Slot machine is on cooldown.",
                StatusCode::TOO_MANY_REQUESTS,
            )),
            SlotsServiceError::InsufficientFunds => Some(PublicError::new(
                "insufficient-funds",
                "Not enough boonbucks to make that bet.",
                StatusCode::UNPROCESSABLE_ENTITY,
            )),
            SlotsServiceError::UserNotFound => Some(PublicError::new(
                "user-not-found",
                "User not found.",
                StatusCode::UNPROCESSABLE_ENTITY,
            )),
            SlotsServiceError::UserService(e) => e.as_public(),
            _ => None,
        }
    }
}

pub struct SlotsService {
    user_repo: BaseRepository<entities::users::Entity>,
    history_repo: BaseRepository<entities::minigame_history::Entity>,
    cooldown_service: Arc<CooldownService>,
    user_service: Arc<UserService>,
    machine: SlotMachine,
}

impl SlotsService {
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
            machine: SlotMachine::new(),
        }
    }

    /// Spin the slot machine for `user_id` with the given `bet`.
    ///
    /// Performs permission, cooldown, and balance checks. On success, the
    /// user's boonbucks are atomically debited by `bet` and credited by
    /// `bet * multiplier`, the cooldown is set, and a row is inserted into
    /// `minigame_history`.
    pub async fn spin(&self, user_id: UserId, bet: i32) -> Result<SpinResult, SlotsServiceError> {
        if !(MIN_BET..=MAX_BET).contains(&bet) {
            return Err(SlotsServiceError::BetOutOfRange);
        }

        self.user_service
            .user_has_permission(user_id, PERM_USE_MINIGAMES)
            .await?;

        let cooldown = SlotCooldown;
        if cooldown
            .on_cooldown(&self.cooldown_service, user_id)
            .await?
        {
            return Err(SlotsServiceError::OnCooldown);
        }

        let (multiplier, reels) = {
            let mut rng = rand::rng();
            self.machine.spin(&mut rng)
        };

        let payout = (bet as i64).saturating_mul(multiplier as i64) as i32;
        let new_balance = self
            .user_repo
            .apply_minigame_outcome(user_id.into(), bet, payout)
            .await?;

        cooldown
            .set_or_replace(&self.cooldown_service, user_id)
            .await?;
        let result_json = serde_json::json!({
            "reels": reels,
            "multiplier": multiplier,
        });
        self.history_repo
            .add(entities::minigame_history::ActiveModel {
                user_id: Set(user_id.into()),
                game: Set(SLOTS_GAME_NAME.to_string()),
                wager: Set(bet),
                payout: Set(payout),
                result: Set(Some(result_json)),
                ..Default::default()
            })
            .await?;

        Ok(SpinResult {
            reels,
            multiplier,
            payout,
            bet,
            new_balance,
            won: multiplier > 0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    fn s(symbol: SlotSymbol) -> ReelSymbol {
        ReelSymbol::Symbol { symbol }
    }

    #[test]
    fn score_three_jackpots_pays_1200() {
        let r = [s(SlotSymbol::Jackpot); 3];
        assert_eq!(score(&r), 1200);
    }

    #[test]
    fn score_three_red_sevens_pays_200() {
        let r = [s(SlotSymbol::RedSeven); 3];
        assert_eq!(score(&r), 200);
    }

    #[test]
    fn score_three_triple_bars_pays_100() {
        let r = [s(SlotSymbol::TripleBar); 3];
        assert_eq!(score(&r), 100);
    }

    #[test]
    fn score_three_double_bars_pays_90() {
        let r = [s(SlotSymbol::DoubleBar); 3];
        assert_eq!(score(&r), 90);
    }

    #[test]
    fn score_three_bars_pays_40() {
        let r = [s(SlotSymbol::Bar); 3];
        assert_eq!(score(&r), 40);
    }

    #[test]
    fn score_three_cherries_pays_40() {
        let r = [s(SlotSymbol::Cherry); 3];
        assert_eq!(score(&r), 40);
    }

    #[test]
    fn score_mixed_bars_pays_10() {
        let r = [
            s(SlotSymbol::Bar),
            s(SlotSymbol::DoubleBar),
            s(SlotSymbol::TripleBar),
        ];
        assert_eq!(score(&r), 10);
    }

    #[test]
    fn score_two_cherries_pays_5() {
        let r = [
            s(SlotSymbol::Cherry),
            s(SlotSymbol::Cherry),
            s(SlotSymbol::Jackpot),
        ];
        assert_eq!(score(&r), 5);
    }

    #[test]
    fn score_one_cherry_pays_1() {
        let r = [
            s(SlotSymbol::Cherry),
            s(SlotSymbol::Jackpot),
            s(SlotSymbol::RedSeven),
        ];
        assert_eq!(score(&r), 1);
    }

    #[test]
    fn score_no_match_loses() {
        let r = [
            ReelSymbol::Blank,
            s(SlotSymbol::Jackpot),
            s(SlotSymbol::RedSeven),
        ];
        assert_eq!(score(&r), 0);
    }

    #[test]
    fn machine_spin_is_deterministic_with_seeded_rng() {
        let machine = SlotMachine::new();
        let mut rng = StdRng::seed_from_u64(42);
        let (m1, r1) = machine.spin(&mut rng);
        let mut rng = StdRng::seed_from_u64(42);
        let (m2, r2) = machine.spin(&mut rng);
        assert_eq!(m1, m2);
        assert_eq!(r1, r2);
    }
}
