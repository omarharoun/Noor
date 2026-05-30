use axum::{
    extract::{Path, Request},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use tower_http::services::ServeDir;
use uuid::Uuid;

use crate::routes;
use crate::state::AppState;

async fn pay_page(Path(id): Path<Uuid>) -> impl IntoResponse {
    let content = tokio::fs::read_to_string("web/pay.html")
        .await
        .unwrap_or_default();
    let html = content.replace("__SESSION_ID__", &id.to_string());
    ([(axum::http::header::CONTENT_TYPE, "text/html")], html)
}

async fn invoice_page(Path(id): Path<Uuid>) -> impl IntoResponse {
    let content = tokio::fs::read_to_string("web/invoice.html")
        .await
        .unwrap_or_default();
    let html = content.replace("__SESSION_ID__", &id.to_string());
    ([(axum::http::header::CONTENT_TYPE, "text/html")], html)
}

async fn admin_handler(req: Request) -> Response {
    let path = req.uri().path();
    let file_path = if path == "/admin" || path.starts_with("/admin/") {
        let relative = path.strip_prefix("/admin").unwrap_or("");
        let clean = if relative.is_empty() || relative == "/" {
            "index.html"
        } else {
            &relative[1..]
        };
        format!("admin/dist/{}", clean)
    } else {
        return StatusCode::NOT_FOUND.into_response();
    };

    match tokio::fs::read_to_string(&file_path).await {
        Ok(content) => {
            let ext = file_path.rsplit('.').next().unwrap_or("");
            let mime = match ext {
                "html" => "text/html",
                "js" => "application/javascript",
                "css" => "text/css",
                "json" => "application/json",
                "svg" => "image/svg+xml",
                "png" => "image/png",
                "ico" => "image/x-icon",
                "woff2" => "font/woff2",
                _ => "text/plain",
            };
            ([(axum::http::header::CONTENT_TYPE, mime)], content).into_response()
        }
        Err(_) => {
            if !file_path.contains('.') || file_path.ends_with(".html") {
                if let Ok(index) = tokio::fs::read_to_string("admin/dist/index.html").await {
                    return ([(axum::http::header::CONTENT_TYPE, "text/html")], index).into_response();
                }
            }
            StatusCode::NOT_FOUND.into_response()
        }
    }
}

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(|| async { "OK" }))
        .route("/api/banks", get(routes::banks::list_banks))
        .route("/api/sessions", get(routes::sessions::list_sessions).post(routes::sessions::create_session))
        .route(
            "/api/sessions/initiate",
            post(routes::initiate::initiate_session),
        )
        .route("/api/sessions/:id", get(routes::sessions::get_session))
        .route("/api/sessions/:id/qr", get(routes::sessions::get_session_qr))
        .route("/api/sessions/:id/stream", get(routes::sessions::stream_session))
        .route("/api/sessions/:id/initiate", post(routes::sessions::initiate_payment))
        .route("/api/sessions/:id/confirm", post(routes::confirm::confirm_payment))
        .route("/api/transactions", get(routes::transactions::list_transactions))
        .route("/api/webhooks/column", post(routes::webhooks::column_webhook))
        .route("/api/webhooks/moderntreasury", post(routes::webhooks::moderntreasury_webhook))
        .route("/api/merchant-api/balances/:merchant_id", get(routes::merchant_api::get_balance))
        .route("/api/merchant-api/merchants/profile", get(routes::merchant_api::get_merchant_profile))
        .route("/api/merchant-api/payments", get(routes::merchant_api::list_payments))
        .route("/api/merchant-api/payments/timeseries", get(routes::merchant_api::payment_timeseries))
        .route("/api/merchant-api/payment_links", post(routes::merchant_api::create_payment_link))
        .route("/api/plaid/link-token", get(routes::plaid::create_link_token))
        .route("/api/plaid/exchange", post(routes::plaid::exchange_public_token))
        .route("/pay/:id", get(pay_page))
        .route("/invoice/:id", get(invoice_page))
        .route("/api/mt/payment-orders/:id", get(routes::mt::get_payment_order))
        .route("/api/mt/counterparties/:id", get(routes::mt::get_counterparty))
        .route("/api/admin/stats", get(routes::admin::get_stats))
        .route("/api/admin/sessions", get(routes::admin::list_sessions))
        .route("/api/admin/sessions/:id", get(routes::admin::get_session_detail))
        .route("/api/admin/merchants", get(routes::admin::list_merchants).post(routes::admin::create_merchant))
        .route("/api/admin/merchants/:id", get(routes::admin::get_merchant).put(routes::admin::update_merchant))
        .route("/api/admin/banks", get(routes::admin::list_banks))
        .route("/api/admin/settlements", get(routes::admin::list_settlements))
        .route("/api/admin/webhooks", get(routes::admin::list_webhooks))
        .route("/admin", get(admin_handler))
        .route("/admin/", get(admin_handler))
        .route("/admin/*path", get(admin_handler))
        .nest_service("/static", ServeDir::new("web"))
        .with_state(state)
}
