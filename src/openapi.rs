use utoipa::{
    Modify, OpenApi,
    openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme},
};

struct DiscordAuthAddon;

impl Modify for DiscordAuthAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme(
                "user_jwt",
                SecurityScheme::Http(
                    HttpBuilder::new()
                        .scheme(HttpAuthScheme::Bearer)
                        .bearer_format("JWT")
                        .build(),
                ),
            );
        }
    }
}

struct UserApiKeyAddon;

impl Modify for UserApiKeyAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme(
                "user_api_key",
                SecurityScheme::Http(
                    HttpBuilder::new()
                        .scheme(HttpAuthScheme::Bearer)
                        .bearer_format("API Key")
                        .build(),
                ),
            );
        }
    }
}

#[derive(OpenApi)]
#[openapi(
    modifiers(&DiscordAuthAddon, &UserApiKeyAddon),
    paths(
        crate::routes::auth::discord_login,
        crate::routes::bears::add_bear,
        crate::routes::bears::get_bear_count,
        crate::routes::cans::add_can,
        crate::routes::cans::get_can_count,
        crate::routes::minigames::dice_roll,
        crate::routes::minigames::pvp_challenge,
        crate::routes::minigames::slots_spin,
        crate::routes::playback::played,
        crate::routes::stream::skip,
        crate::routes::stream::volume,
        crate::routes::stream::queue_push,
        crate::routes::stream::playlist_reload,
        crate::routes::stream::raw,
        crate::routes::user::me,
        crate::routes::user::pay,
    )
)]
pub struct ApiDoc;
