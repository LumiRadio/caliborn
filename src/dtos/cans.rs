use axum::response::IntoResponse;
use serde::Serialize;
use utoipa::ToSchema;

use crate::dtos::json;

#[derive(Serialize, ToSchema)]
#[schema(
    examples(json!({"count": 10, "cooldown": 35}))
)]
pub struct CanCountDto {
    pub count: u64,
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub cooldown_info: Option<CanCooldownInfo>,
}

impl IntoResponse for CanCountDto {
    fn into_response(self) -> axum::response::Response {
        json(self).into_response()
    }
}

#[derive(Serialize, ToSchema)]
pub struct CanCooldownInfo {
    pub cooldown: u64,
}
