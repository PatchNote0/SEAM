use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

#[derive(Deserialize)]
struct NewEntry {
    account_id: i64,
    amount_cents: i64,
    currency: String,
}

#[derive(Serialize)]
struct EntryCreated {
    entry_id: i64,
}

#[derive(Serialize)]
struct Balance {
    account_id: i64,
    holder: String,
    balance_cents: i64,
}

#[tokio::main]
async fn main() {
    let database_url = std::env::var("LEDGER_DB_URL").expect("LEDGER_DB_URL must be set");
    let pool = PgPool::connect(&database_url)
        .await
        .expect("the ledger database is reachable");

    let app = Router::new()
        .route("/ledger/entries", post(create_entry))
        .route("/ledger/accounts/{account_id}/balance", get(balance))
        .with_state(pool);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080")
        .await
        .expect("port 8080 is free");
    axum::serve(listener, app).await.expect("the server runs");
}

async fn create_entry(
    State(pool): State<PgPool>,
    Json(entry): Json<NewEntry>,
) -> Result<Json<EntryCreated>, StatusCode> {
    let (entry_id,): (i64,) = sqlx::query_as(
        "INSERT INTO ledger_entries (account_id, amount_cents, currency) \
         VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(entry.account_id)
    .bind(entry.amount_cents)
    .bind(&entry.currency)
    .fetch_one(&pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(EntryCreated { entry_id }))
}

async fn balance(
    State(pool): State<PgPool>,
    Path(account_id): Path<i64>,
) -> Result<Json<Balance>, StatusCode> {
    let (holder, balance_cents): (String, i64) = sqlx::query_as(
        "SELECT a.name, COALESCE(SUM(e.amount_cents), 0)::BIGINT \
         FROM accounts a LEFT JOIN ledger_entries e ON e.account_id = a.id \
         WHERE a.id = $1 GROUP BY a.name",
    )
    .bind(account_id)
    .fetch_one(&pool)
    .await
    .map_err(|_| StatusCode::NOT_FOUND)?;
    Ok(Json(Balance { account_id, holder, balance_cents }))
}
