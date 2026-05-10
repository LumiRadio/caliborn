use axum::Router;

use crate::{AppState, services::auth::authenticate};

pub mod cooldowns;
pub mod permissions;
pub mod slcb;
pub mod users;

pub fn routes(state: AppState) -> Router<AppState> {
    Router::new()
        .merge(users::routes())
        .merge(permissions::routes())
        .merge(cooldowns::routes())
        .merge(slcb::routes())
        .layer(axum::middleware::from_fn_with_state(state, authenticate))
}
