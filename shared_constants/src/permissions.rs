use std::collections::HashMap;

use sea_orm::sea_query::SimpleExpr;

#[derive(Debug, Clone, Copy)]
pub struct Permission {
    pub name: &'static str,
    pub description: &'static str,
}

impl Permission {
    pub const fn new(name: &'static str, description: &'static str) -> Self {
        Self { name, description }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        ALL_PERMISSIONS_BY_NAME.get(name).cloned()
    }

    pub fn into_values(self) -> [SimpleExpr; 2] {
        [self.name.into(), self.description.into()]
    }
}

impl IntoIterator for Permission {
    type Item = SimpleExpr;
    type IntoIter =
        std::iter::Map<std::array::IntoIter<SimpleExpr, 2>, fn(SimpleExpr) -> SimpleExpr>;

    fn into_iter(self) -> Self::IntoIter {
        self.into_values().into_iter().map(|s| s.into())
    }
}

pub const PERM_MANAGE_USERS: Permission = Permission::new("manage_users", "Manage users");
pub const PERM_USE_MINIGAMES: Permission = Permission::new("use_minigames", "Use minigames");
pub const PERM_USE_WEB_CHAT: Permission = Permission::new("use_web_chat", "Use web chat");
pub const PERM_USE_BOT: Permission = Permission::new("use_bot", "Use bot");
pub const PERM_MANAGE_STREAM: Permission = Permission::new("manage_stream", "Manage stream");
pub const PERM_MANAGE_ACTIVITY_ROLES: Permission =
    Permission::new("manage_activity_roles", "Manage activity roles");

pub const ALL_PERMISSIONS: &[Permission] = &[
    PERM_MANAGE_USERS,
    PERM_USE_MINIGAMES,
    PERM_USE_WEB_CHAT,
    PERM_USE_BOT,
    PERM_MANAGE_STREAM,
    PERM_MANAGE_ACTIVITY_ROLES,
];
pub static ALL_PERMISSIONS_BY_NAME: once_cell::sync::Lazy<HashMap<&'static str, Permission>> =
    once_cell::sync::Lazy::new(|| ALL_PERMISSIONS.iter().map(|p| (p.name, *p)).collect());
