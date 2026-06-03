use axum::{
    extract::{DefaultBodyLimit, Path, Request, State},
    http::{header, HeaderName, HeaderValue, Method, StatusCode},
    middleware,
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::services::ServeDir;
use uuid::Uuid;

use crate::auth;
use crate::routes;
use crate::state::AppState;

/// Readiness probe — verifies the DB is actually reachable, unlike /health
/// which only proves the process is up.
async fn readiness(State(state): State<AppState>) -> Response {
    match sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(&state.db.pool)
        .await
    {
        Ok(_) => (StatusCode::OK, "ready").into_response(),
        Err(_) => (StatusCode::SERVICE_UNAVAILABLE, "db unavailable").into_response(),
    }
}

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
                    return ([(axum::http::header::CONTENT_TYPE, "text/html")], index)
                        .into_response();
                }
            }
            StatusCode::NOT_FOUND.into_response()
        }
    }
}

pub fn build_router(state: AppState) -> Router {
    // Operator console API — gated behind a valid admin JWT (P0: was fully open
    // and leaked password_hash/api_key). Login/me are public (see `public`).
    let admin = Router::new()
        .route("/api/admin/stats", get(routes::admin::get_stats))
        .route("/api/admin/sessions", get(routes::admin::list_sessions))
        .route(
            "/api/admin/sessions/:id",
            get(routes::admin::get_session_detail),
        )
        .route(
            "/api/admin/merchants",
            get(routes::admin::list_merchants).post(routes::admin::create_merchant),
        )
        .route(
            "/api/admin/merchants/:id",
            get(routes::admin::get_merchant).put(routes::admin::update_merchant),
        )
        .route("/api/admin/banks", get(routes::admin::list_banks))
        .route(
            "/api/admin/settlements",
            get(routes::admin::list_settlements),
        )
        .route("/api/admin/webhooks", get(routes::admin::list_webhooks))
        .route(
            "/api/admin/webhooks/:id/retry",
            post(routes::admin::retry_webhook),
        )
        .route("/api/admin/health", get(routes::admin::get_health))
        .route("/api/admin/payouts", get(routes::admin::list_all_payouts))
        .route("/api/admin/invoices", get(routes::admin::list_all_invoices))
        .route("/api/admin/deposits", get(routes::admin::list_all_deposits))
        .route("/api/admin/audit", get(routes::admin::get_audit))
        .route(
            "/api/admin/operators",
            get(routes::admin::list_operators).post(routes::admin::create_operator),
        )
        .route(
            "/api/admin/operators/:id",
            axum::routing::patch(routes::admin::update_operator),
        )
        .route(
            "/api/admin/merchants/:id/users",
            post(routes::admin::create_merchant_user),
        )
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth::admin_auth,
        ));

    // Operator surface: session creation/listing, money initiation and the
    // transactions ledger. P0: these were PUBLIC — anyone could move money for
    // any session UUID or read any merchant's data by passing merchant_id. Now
    // gated behind the operator JWT (the admin console attaches it).
    let operator = Router::new()
        .route(
            "/api/sessions",
            get(routes::sessions::list_sessions).post(routes::sessions::create_session),
        )
        .route(
            "/api/sessions/initiate",
            post(routes::initiate::initiate_session),
        )
        .route(
            "/api/sessions/:id/initiate",
            post(routes::sessions::initiate_payment),
        )
        .route(
            "/api/transactions",
            get(routes::transactions::list_transactions),
        )
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth::admin_auth,
        ));

    // Merchant API — gated behind a valid merchant API key (injects merchant id).
    let merchant = Router::new()
        .route(
            "/api/merchant-api/balances/:merchant_id",
            get(routes::merchant_api::get_balance),
        )
        .route(
            "/api/merchant-api/merchants/profile",
            get(routes::merchant_api::get_merchant_profile)
                .put(routes::merchant_api::update_merchant_profile),
        )
        .route(
            "/api/merchant-api/payments",
            get(routes::merchant_api::list_payments),
        )
        .route(
            "/api/merchant-api/payments/timeseries",
            get(routes::merchant_api::payment_timeseries),
        )
        .route(
            "/api/merchant-api/payment_links",
            post(routes::merchant_api::create_payment_link),
        )
        .route(
            "/api/merchant-api/bank-account",
            get(routes::merchant_api::get_bank_account)
                .post(routes::merchant_api::add_bank_account),
        )
        .route(
            "/api/merchant-api/invoices",
            get(routes::merchant_api::list_invoices).post(routes::merchant_api::create_invoice),
        )
        .route(
            "/api/merchant-api/invoices/:id",
            get(routes::merchant_api::get_invoice),
        )
        .route(
            "/api/merchant-api/recurring-invoices",
            get(routes::merchant_api::list_recurring).post(routes::merchant_api::create_recurring),
        )
        .route(
            "/api/merchant-api/recurring-invoices/:id/cancel",
            post(routes::merchant_api::cancel_recurring),
        )
        .route(
            "/api/merchant-api/users",
            get(routes::merchant_api::list_users).post(routes::merchant_api::invite_user),
        )
        .route(
            "/api/merchant-api/payouts",
            get(routes::merchant_api::list_payouts).post(routes::merchant_api::create_payout),
        )
        .route(
            "/api/merchant-api/payouts/:id/approve",
            post(routes::merchant_api::approve_payout),
        )
        .route(
            "/api/merchant-api/payouts/:id/reject",
            post(routes::merchant_api::reject_payout),
        )
        .route(
            "/api/merchant-api/balance/add-funds",
            post(routes::merchant_api::add_funds),
        )
        .route(
            "/api/merchant-api/balance/withdraw",
            post(routes::merchant_api::withdraw),
        )
        .route(
            "/api/merchant-api/deposits",
            get(routes::merchant_api::list_deposits),
        )
        .route(
            "/api/merchant-api/reports/summary",
            get(routes::merchant_api::report_summary),
        )
        .route(
            "/api/merchant-api/statement",
            get(routes::merchant_api::statement),
        )
        .route(
            "/api/merchant-api/notifications",
            get(routes::merchant_api::list_notifications),
        )
        .route(
            "/api/merchant-api/notifications/read",
            post(routes::merchant_api::mark_notifications_read),
        )
        .route(
            "/api/merchant-api/payees",
            get(routes::merchant_api::list_payees).post(routes::merchant_api::create_payee),
        )
        .route(
            "/api/merchant-api/payees/:id",
            axum::routing::delete(routes::merchant_api::delete_payee),
        )
        .route(
            "/api/merchant-api/customers",
            get(routes::merchant_api::list_customers).post(routes::merchant_api::create_customer),
        )
        .route(
            "/api/merchant-api/customers/:id",
            axum::routing::delete(routes::merchant_api::delete_customer),
        )
        .route(
            "/api/merchant-api/exports/:kind",
            get(routes::merchant_api::export_csv),
        )
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth::merchant_auth,
        ));

    // Public surfaces: health, auth bootstrap, webhooks (signature-verified),
    // and the customer-facing pay/session flow reached via unguessable UUIDs.
    let public = Router::new()
        .route("/health", get(|| async { "OK" }))
        .route("/health/ready", get(readiness))
        .route("/api/admin/login", post(auth::login))
        .route("/api/admin/me", get(auth::me))
        .route("/api/merchant/login", post(auth::merchant_login))
        .route("/api/merchant/me", get(auth::merchant_me))
        .route("/api/banks", get(routes::banks::list_banks))
        // Customer-facing, capability-scoped by the unguessable session UUID.
        // `get_session` returns a REDACTED view (no raw bank account/routing).
        .route("/api/sessions/:id", get(routes::sessions::get_session))
        .route(
            "/api/sessions/:id/qr",
            get(routes::sessions::get_session_qr),
        )
        .route(
            "/api/sessions/:id/stream",
            get(routes::sessions::stream_session),
        )
        .route(
            "/api/sessions/:id/confirm",
            post(routes::confirm::confirm_payment),
        )
        .route(
            "/api/webhooks/column",
            post(routes::webhooks::column_webhook),
        )
        .route(
            "/api/webhooks/moderntreasury",
            post(routes::webhooks::moderntreasury_webhook),
        )
        .route("/pay/:id", get(pay_page))
        .route("/invoice/:id", get(invoice_page))
        .route(
            "/api/mt/payment-orders/:id",
            get(routes::mt::get_payment_order),
        )
        .route(
            "/api/mt/counterparties/:id",
            get(routes::mt::get_counterparty),
        )
        .route("/admin", get(admin_handler))
        .route("/admin/", get(admin_handler))
        .route("/admin/*path", get(admin_handler))
        .nest_service("/static", ServeDir::new("web"));

    // Restrict CORS to configured origins (CORS_ALLOWED_ORIGINS, comma-separated;
    // defaults to PUBLIC_APP_URL). Bearer tokens travel in the Authorization
    // header (not cookies), so credentials stay off; a 256 KiB body cap guards
    // the JSON handlers.
    let mut origins: Vec<HeaderValue> = std::env::var("CORS_ALLOWED_ORIGINS")
        .ok()
        .map(|s| {
            s.split(',')
                .filter_map(|o| HeaderValue::from_str(o.trim()).ok())
                .collect()
        })
        .unwrap_or_default();
    if origins.is_empty() {
        if let Ok(hv) = HeaderValue::from_str(&state.config.public_app_url) {
            origins.push(hv);
        }
    }
    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins))
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
        ])
        .allow_headers([
            header::AUTHORIZATION,
            header::CONTENT_TYPE,
            HeaderName::from_static("x-api-key"),
            HeaderName::from_static("idempotency-key"),
        ]);

    public
        .merge(admin)
        .merge(operator)
        .merge(merchant)
        .layer(DefaultBodyLimit::max(256 * 1024))
        .layer(cors)
        .with_state(state)
}
