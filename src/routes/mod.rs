/// Admin API routes (RBAC-gated user/role/cooldown/slcb management).
pub mod admin;
/// Authentication routes
pub mod auth;
/// Bear routes
pub mod bears;
/// Can routes
pub mod cans;
/// Minigame routes
pub mod minigames;
/// Inbound playback ingest (Liquidsoap → Caliborn)
pub mod playback;
/// Song routes
pub mod songs;
/// Stream control routes (Caliborn → Liquidsoap)
pub mod stream;
/// User routes
pub mod user;
/// WebSocket realtime endpoint
pub mod ws;
