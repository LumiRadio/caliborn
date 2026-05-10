use std::collections::HashMap;

use sea_orm::sea_query::SimpleExpr;

#[derive(Debug, Clone, Copy, Eq)]
pub struct Permission {
    pub name: &'static str,
    pub description: &'static str,
    pub built_in: bool,
}

impl Permission {
    pub const fn new(name: &'static str, description: &'static str, built_in: bool) -> Self {
        Self {
            name,
            description,
            built_in,
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        ALL_PERMISSIONS_BY_NAME.get(name).cloned()
    }

    pub fn into_values(self) -> [SimpleExpr; 3] {
        [
            self.name.into(),
            self.description.into(),
            self.built_in.into(),
        ]
    }
}

impl IntoIterator for Permission {
    type Item = SimpleExpr;
    type IntoIter =
        std::iter::Map<std::array::IntoIter<SimpleExpr, 3>, fn(SimpleExpr) -> SimpleExpr>;

    fn into_iter(self) -> Self::IntoIter {
        self.into_values().into_iter().map(|s| s.into())
    }
}

impl PartialEq<Permission> for Permission {
    fn eq(&self, other: &Permission) -> bool {
        self.name == other.name
    }
}

pub const PERM_MANAGE_USERS: Permission = Permission::new("manage_users", "Manage users", true);
pub const PERM_USE_MINIGAMES: Permission = Permission::new("use_minigames", "Use minigames", true);
pub const PERM_USE_WEB_CHAT: Permission = Permission::new("use_web_chat", "Use web chat", true);
pub const PERM_USE_BOT: Permission = Permission::new(
    "use_bot",
    "Use other radio features (like song requests)",
    true,
);
pub const PERM_MANAGE_STREAM: Permission = Permission::new("manage_stream", "Manage stream", true);
pub const PERM_MANAGE_ACTIVITY_ROLES: Permission =
    Permission::new("manage_activity_roles", "Manage activity roles", true);
pub const PERM_MANAGE_PERMISSIONS: Permission =
    Permission::new("manage_permissions", "Manage roles and permissions", true);
pub const PERM_MANAGE_COOLDOWNS: Permission =
    Permission::new("manage_cooldowns", "Manage user/global cooldowns", true);
pub const PERM_MANAGE_SLCB: Permission = Permission::new(
    "manage_slcb",
    "Manage SLCB legacy data imports and matches",
    true,
);

pub const ALL_PERMISSIONS: &[Permission] = &[
    PERM_MANAGE_USERS,
    PERM_USE_MINIGAMES,
    PERM_USE_WEB_CHAT,
    PERM_USE_BOT,
    PERM_MANAGE_STREAM,
    PERM_MANAGE_ACTIVITY_ROLES,
    PERM_MANAGE_PERMISSIONS,
    PERM_MANAGE_COOLDOWNS,
    PERM_MANAGE_SLCB,
];
pub static ALL_PERMISSIONS_BY_NAME: once_cell::sync::Lazy<HashMap<&'static str, Permission>> =
    once_cell::sync::Lazy::new(|| ALL_PERMISSIONS.iter().map(|p| (p.name, *p)).collect());
