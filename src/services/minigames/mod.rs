//! Server-authoritative minigames.
//!
//! All RNG, payouts, and balance updates run inside Caliborn so that any
//! client (Calliope, Discord bot, future surfaces) sees identical results.

use crate::{
    ServiceRegistry,
    repositories::{AlwaysCloneableConnection, BaseRepository},
};

pub mod dice;
pub mod pvp;
pub mod slots;

pub use dice::DiceService;
pub use pvp::PvpService;
pub use slots::SlotsService;

/// Aggregator over per-game services so the registry exposes a single
/// `MinigameService` rather than one entry per game.
pub struct MinigameService {
    pub slots: SlotsService,
    pub dice: DiceService,
    pub pvp: PvpService,
}

impl MinigameService {
    pub fn new(db: &AlwaysCloneableConnection, registry: &ServiceRegistry) -> Self {
        Self {
            slots: SlotsService::new(
                BaseRepository::new(db),
                BaseRepository::new(db),
                registry.cooldown_service(),
                registry.user_service(),
            ),
            dice: DiceService::new(db, registry.cooldown_service(), registry.user_service()),
            pvp: PvpService::new(
                BaseRepository::new(db),
                BaseRepository::new(db),
                registry.cooldown_service(),
                registry.user_service(),
            ),
        }
    }
}
