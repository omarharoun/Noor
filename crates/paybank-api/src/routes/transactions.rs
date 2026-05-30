use crate::state::AppState;
use axum::{
    extract::{Query, State},
    Json,
};
use paybank_core::AppError;
use paybank_db::transaction_repo;
use serde::Deserialize;
use uuid::Uuid;

#[derive(Deserialize)]
pub struct TransactionsQuery {
    pub merchant_id: Uuid,
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

fn default_limit() -> i64 {
    20
}

pub async fn list_transactions(
    State(state): State<AppState>,
    Query(params): Query<TransactionsQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    let (transactions, total) = transaction_repo::list_transactions(
        &state.db.pool,
        params.merchant_id,
        params.limit,
        params.offset,
    )
    .await?;

    let txns = transactions
        .iter()
        .map(|t| {
            serde_json::json!({
                "id": t.id, "session_id": t.session_id, "reference": t.reference,
                "amount_cents": t.amount_cents, "currency": "USD",
                "rail_used": t.rail_used, "column_ref": t.column_ref,
                "settled_at": t.settled_at, "created_at": t.created_at,
            })
        })
        .collect::<Vec<_>>();

    Ok(Json(serde_json::json!({
        "transactions": txns,
        "total": total
    })))
}