pub use sea_orm_migration::prelude::*;

mod m20240506_215517_initial;
mod m20240530_174050_edit_users_change_watched_hours;
mod m20240706_153336_edit_server_channel_config_add_last_message_sent;
mod m20250420_155859_edit_users_add_username;
mod m20250422_120742_create_api_keys;
mod m20250423_082923_edit_users_remove_grist;
mod m20250424_162423_edit_api_keys_add_hash;
mod m20250429_074227_create_cooldowns;
mod m20250702_192327_create_songs_fulltext;
mod m20250715_104753_create_roles;
mod m20250715_104843_create_permissions;
mod m20250715_105012_create_role_permissions;
mod m20250715_105431_create_user_permissions;
mod m20250715_105638_seed_roles;
mod m20250715_111004_seed_permissions;
mod m20250715_164729_seed_role_permissions;
mod m20251127_103126_edit_users_add_role_id;
mod m20260510_120000_create_radio_state;
mod m20260510_120100_create_minigame_history;
mod m20260510_120200_create_discord_role_connections;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20240506_215517_initial::Migration),
            Box::new(m20240530_174050_edit_users_change_watched_hours::Migration),
            Box::new(m20240706_153336_edit_server_channel_config_add_last_message_sent::Migration),
            Box::new(m20250420_155859_edit_users_add_username::Migration),
            Box::new(m20250422_120742_create_api_keys::Migration),
            Box::new(m20250423_082923_edit_users_remove_grist::Migration),
            Box::new(m20250424_162423_edit_api_keys_add_hash::Migration),
            Box::new(m20250429_074227_create_cooldowns::Migration),
            Box::new(m20250702_192327_create_songs_fulltext::Migration),
            Box::new(m20250715_104753_create_roles::Migration),
            Box::new(m20250715_104843_create_permissions::Migration),
            Box::new(m20250715_105012_create_role_permissions::Migration),
            Box::new(m20250715_105431_create_user_permissions::Migration),
            Box::new(m20250715_105638_seed_roles::Migration),
            Box::new(m20250715_111004_seed_permissions::Migration),
            Box::new(m20250715_164729_seed_role_permissions::Migration),
            Box::new(m20251127_103126_edit_users_add_role_id::Migration),
            Box::new(m20260510_120000_create_radio_state::Migration),
            Box::new(m20260510_120100_create_minigame_history::Migration),
            Box::new(m20260510_120200_create_discord_role_connections::Migration),
        ]
    }
}
