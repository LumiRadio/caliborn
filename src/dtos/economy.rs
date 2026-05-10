//! Economy-related data transfer objects.

use axum::response::IntoResponse;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::json;

/// Request body for `POST /user/me/pay`.
#[derive(Deserialize, ToSchema)]
#[schema(
    description = "Pay boonbucks to another user",
    examples(json!({"recipient_id": 675674657_i64, "amount": 50}))
)]
pub struct PayRequest {
    /// Discord user ID of the recipient.
    #[schema(example = 675674657_i64)]
    pub recipient_id: i64,

    /// Amount of boonbucks to send. Must be > 0.
    #[schema(example = 50)]
    pub amount: i32,
}

/// Response body for a successful pay request.
#[derive(Serialize, ToSchema)]
#[schema(
    description = "Result of a successful pay transfer",
    examples(json!({
        "amount": 50,
        "sender_balance": 950,
        "recipient_balance": 1050
    }))
)]
pub struct PayResponse {
    /// Amount transferred.
    pub amount: i32,
    /// Sender's balance after the transfer.
    pub sender_balance: i32,
    /// Recipient's balance after the transfer.
    pub recipient_balance: i32,
}

impl IntoResponse for PayResponse {
    fn into_response(self) -> axum::response::Response {
        json(self).into_response()
    }
}
