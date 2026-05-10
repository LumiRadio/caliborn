//! `SeaORM` Entity, hand-written to match migration `m20260510_120300_create_discord_oauth_tokens`.

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "discord_oauth_tokens")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub user_id: i64,
    pub access_token_ciphertext: Vec<u8>,
    pub access_token_nonce: Vec<u8>,
    pub refresh_token_ciphertext: Vec<u8>,
    pub refresh_token_nonce: Vec<u8>,
    pub access_expires_at: DateTime,
    pub scopes: String,
    pub updated_at: DateTime,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::UserId",
        to = "super::users::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    Users,
}

impl Related<super::users::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Users.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
