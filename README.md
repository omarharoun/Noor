# NoorPay — Payment Operations Monolith

A full-stack payment processing platform with a Rust API backend, Refine + Ant Design admin dashboard, and integrations with Modern Treasury, Column, and Plaid.

---

## Table of Contents

- [Architecture](#architecture)
- [Tech Stack](#tech-stack)
- [Database Schema](#database-schema)
- [API Routes](#api-routes)
- [External Integrations](#external-integrations)
- [Payment Flow](#payment-flow)
- [Admin Dashboard](#admin-dashboard)
- [Prerequisites](#prerequisites)
- [Setup](#setup)
- [Development](#development)
- [Deployment](#deployment)
- [Environment Variables](#environment-variables)

---

## Architecture

```
┌─────────────┐     ┌──────────────────────────────────────┐
│  Browser     │     │  Rust API (axum) :8888               │
│  (Pay Page)  │────▶│                                      │
│  /pay/:id    │     │  ┌─────────┐  ┌──────────────────┐  │
└─────────────┘     │  │  Routes  │──▶  Admin Repo      │  │
                    │  │          │  │  Session Repo    │  │
┌─────────────┐     │  │  Auth    │  │  Bank Repo       │──▶  PostgreSQL
│  Admin UI   │     │  │  Webhooks│  │  Transaction Repo│  │  :5433
│  /admin/*   │────▶│  │  Plaid   │  │  Ledger Repo     │  │
│  Refine+AntD│     │  │  MT Proxy│  │  Idempotency Repo│  │
└─────────────┘     │  └─────────┘  └──────────────────┘  │
                    │                                      │
                    │  ┌──────────────────────────────┐    │
                    │  │  Payment Providers           │    │
                    │  │  ┌──────────┐  ┌─────────┐   │    │
                    │  │  │Mod. Treas│  │ Column  │   │    │
                    │  │  └──────────┘  └─────────┘   │    │
                    │  │  ┌──────────┐                │    │
                    │  │  │  Plaid   │                │    │
                    │  │  └──────────┘                │    │
                    │  └──────────────────────────────┘    │
                    └──────────────────────────────────────┘
```

---

## Tech Stack

| Layer | Technology |
|---|---|
| **API** | Rust, axum, tokio, sqlx, serde |
| **Database** | PostgreSQL 16 |
| **Admin Dashboard** | React 18, Refine v4, Ant Design 5, recharts |
| **Payment Provider** | Modern Treasury (primary) / Column (fallback) |
| **Bank Linking** | Plaid (auth product) |
| **Containerization** | Docker, multi-stage build |
| **QR Codes** | `qrcode` crate |

---

## Database Schema

### `merchants`
| Column | Type | Notes |
|---|---|---|
| `id` | `UUID` | PK, `gen_random_uuid()` |
| `name` | `TEXT` | NOT NULL |
| `email` | `TEXT` | NOT NULL, UNIQUE |
| `password_hash` | `TEXT` | nullable |
| `api_key` | `TEXT` | NOT NULL, UNIQUE |
| `webhook_url` | `TEXT` | nullable |
| `webhook_secret` | `TEXT` | nullable |
| `business_name` | `TEXT` | nullable |
| `business_type` | `TEXT` | nullable |
| `registration_number` | `TEXT` | nullable |
| `tax_id` | `TEXT` | nullable |
| `industry_category` | `TEXT` | nullable |
| `website_url` | `TEXT` | nullable |
| `status` | `TEXT` | DEFAULT `'pending'` |
| `kyc_status` | `TEXT` | DEFAULT `'not_started'` |
| `risk_level` | `TEXT` | DEFAULT `'medium'` |
| `column_account_id` | `TEXT` | nullable |
| `hyperswitch_merchant_id` | `TEXT` | nullable |
| `onboarding_completed_at` | `TIMESTAMPTZ` | nullable |
| `created_at` | `TIMESTAMPTZ` | DEFAULT `NOW()` |
| `updated_at` | `TIMESTAMPTZ` | DEFAULT `NOW()` |

### `banks`
| Column | Type | Notes |
|---|---|---|
| `id` | `TEXT` | PK |
| `name` | `TEXT` | NOT NULL |
| `routing_number` | `TEXT` | nullable |
| `supports_fednow` | `BOOLEAN` | DEFAULT FALSE |
| `supports_rtp` | `BOOLEAN` | DEFAULT FALSE |
| `supports_wire` | `BOOLEAN` | DEFAULT FALSE |
| `logo_url` | `TEXT` | nullable |
| `display_order` | `INT` | DEFAULT 999 |

### `payment_sessions`
| Column | Type | Notes |
|---|---|---|
| `id` | `UUID` | PK |
| `merchant_id` | `UUID` | FK → `merchants(id)` |
| `bank_id` | `TEXT` | FK → `banks(id)` |
| `amount_cents` | `BIGINT` | NOT NULL |
| `currency` | `TEXT` | DEFAULT `'USD'` |
| `note` | `TEXT` | nullable |
| `status` | `TEXT` | DEFAULT `'pending'` |
| `rail_used` | `TEXT` | nullable |
| `column_ref` | `TEXT` | nullable |
| `column_counterparty_id` | `TEXT` | nullable |
| `customer_name` through `customer_account_type` | various | nullable |
| `expires_at` | `TIMESTAMPTZ` | NOT NULL |
| `provider` | `VARCHAR(50)` | nullable |
| `created_at` / `updated_at` | `TIMESTAMPTZ` | DEFAULT `NOW()` |

### `transactions`
| Column | Type | Notes |
|---|---|---|
| `id` | `UUID` | PK |
| `session_id` | `UUID` | FK → `payment_sessions(id)` |
| `merchant_id` | `UUID` | FK → `merchants(id)` |
| `amount_cents` | `BIGINT` | NOT NULL |
| `rail_used` | `TEXT` | NOT NULL |
| `column_ref` | `TEXT` | nullable |
| `reference` | `TEXT` | UNIQUE |
| `settled_at` | `TIMESTAMPTZ` | NOT NULL |
| `provider` | `VARCHAR(50)` | nullable |
| `created_at` | `TIMESTAMPTZ` | DEFAULT `NOW()` |

### `payment_attempts`
| Column | Type | Notes |
|---|---|---|
| `id` | `UUID` | PK |
| `session_id` | `UUID` | FK → `payment_sessions(id)` CASCADE |
| `rail` | `TEXT` | NOT NULL |
| `provider_ref` | `TEXT` | nullable |
| `request_payload` / `response_body` | `JSONB` | nullable |
| `response_status` | `INT` | nullable |
| `error_message` | `TEXT` | nullable |
| `status` | `TEXT` | NOT NULL |
| `created_at` / `updated_at` | `TIMESTAMPTZ` | DEFAULT `NOW()` |

### `ledger_accounts`, `journal_entries`, `ledger_postings`
Double-entry accounting tables. Ledger accounts track asset/revenue/liability accounts per merchant. Journal entries and postings record every financial movement with debits and credits.

### `idempotency_keys`
Idempotency support via `(merchant_id, idempotency_key)` composite PK with request hashing.

### `webhook_events`, `webhook_delivery_logs`
Webhook event queue with retry tracking, delivery logging, and indexed query support for pending/failed events.

### Foreign Key Relationships

```
merchants (id) ◀── payment_sessions (merchant_id)
merchants (id) ◀── transactions (merchant_id)
merchants (id) ◀── ledger_accounts (merchant_id)
merchants (id) ◀── webhook_events (merchant_id)

banks (id) ◀── payment_sessions (bank_id)

payment_sessions (id) ◀── transactions (session_id)
payment_sessions (id) ◀── payment_attempts (session_id) [CASCADE]
payment_sessions (id) ◀── journal_entries (session_id) [SET NULL]
payment_sessions (id) ◀── webhook_events (session_id) [SET NULL]

journal_entries (id) ◀── ledger_postings (journal_entry_id) [CASCADE]
ledger_accounts (id) ◀── ledger_postings (account_id)

webhook_events (id) ◀── webhook_delivery_logs (webhook_event_id) [CASCADE]
```

### Seed Data

Migration `002_seed_data.sql` inserts:

- **2 merchants** (Demo Merchant with api_key `pb_test_1234567890abcdef`, Demo Merchant 2)
- **10 banks** (Chase, BofA, Wells Fargo, Citibank, Capital One, US Bank, PNC, TD, Navy Federal, Ally)
- **4 ledger accounts** for Demo Merchant: Cash (asset), Receivables (asset), Revenue (revenue), Settlement (liability)

---

## API Routes

### Public / Payment

| Method | Path | Description |
|---|---|---|
| `GET` | `/health` | Health check |
| `GET` | `/api/banks` | List all banks |
| `GET` | `/api/sessions` | List sessions (query: `merchant_id`, `limit`, `offset`) |
| `POST` | `/api/sessions` | Create payment session |
| `POST` | `/api/sessions/initiate` | Initiate payment for an authorized session |
| `GET` | `/api/sessions/:id` | Get session detail |
| `GET` | `/api/sessions/:id/qr` | Get QR code SVG for pay page |
| `GET` | `/api/sessions/:id/stream` | SSE stream of session status updates |
| `POST` | `/api/sessions/:id/confirm` | Confirm payment + create counterparty |
| `POST` | `/api/sessions/:id/initiate` | Initiate transfer for session |
| `GET` | `/api/transactions` | List transactions |
| `GET` | `/pay/:id` | Customer payment page |
| `GET` | `/invoice/:id` | Invoice page |

### Plaid

| Method | Path | Description |
|---|---|---|
| `GET` | `/api/plaid/link-token` | Create Plaid Link token |
| `POST` | `/api/plaid/exchange` | Exchange public token for access token |

### Webhooks

| Method | Path | Description |
|---|---|---|
| `POST` | `/api/webhooks/column` | Column webhook receiver (HMAC-verified) |
| `POST` | `/api/webhooks/moderntreasury` | Modern Treasury webhook receiver |

### Merchant API

| Method | Path | Description |
|---|---|---|
| `GET` | `/api/merchant-api/balances/:merchant_id` | Get merchant balance |
| `GET` | `/api/merchant-api/merchants/profile` | Get merchant profile |
| `GET` | `/api/merchant-api/payments` | List merchant payments |
| `GET` | `/api/merchant-api/payments/timeseries` | Payment time series |
| `POST` | `/api/merchant-api/payment_links` | Create payment link |

### Modern Treasury Proxy

| Method | Path | Description |
|---|---|---|
| `GET` | `/api/mt/payment-orders/:id` | Get MT payment order |
| `GET` | `/api/mt/counterparties/:id` | Get MT counterparty |

### Admin

| Method | Path | Description |
|---|---|---|
| `GET` | `/api/admin/stats` | Dashboard statistics (today volume, pending, 7-day trend) |
| `GET` | `/api/admin/sessions` | Filtered/paginated sessions |
| `GET` | `/api/admin/sessions/:id` | Session detail |
| `GET` | `/api/admin/merchants` | List merchants |
| `POST` | `/api/admin/merchants` | Create merchant |
| `GET` | `/api/admin/merchants/:id` | Get merchant |
| `PUT` | `/api/admin/merchants/:id` | Update merchant |
| `GET` | `/api/admin/banks` | List banks |
| `GET` | `/api/admin/settlements` | Aggregated settlements by merchant |
| `GET` | `/api/admin/webhooks` | Paginated webhook events |
| `GET` | `/admin`, `/admin/`, `/admin/*` | Admin SPA (client-side routing) |

---

## External Integrations

### Modern Treasury (Primary Provider)

- **Base URL:** `https://app.moderntreasury.com/api`
- **Auth:** HTTP Basic (`org_id:api_key`)
- **Endpoints:** Create counterparty, create payment orders, get payment order/counterparty details
- **Payment rails:** ACH, Wire, RTP
- Activated when `MODERN_TREASURY_API_KEY`, `MODERN_TREASURY_ORG_ID`, and `MT_INTERNAL_ACCOUNT_ID` are set.

### Column (Fallback Provider)

- **Base URL:** `https://api.column.com`
- **Auth:** HTTP Basic (`:api_key`)
- **Endpoints:** Create entities, accounts, transfers; get transfer status
- **Payment rails:** FedNow, ACH, Wire, RTP
- Used when Modern Treasury env vars are not set.

### Plaid

- **Base URL:** `https://sandbox.plaid.com`
- **Endpoints:** Create link token, exchange public token, get auth data
- **Product:** `auth` (retrieves account and routing numbers)
- **Timeout:** 30 seconds

### Rail Selection Logic

For each payment session, the best available rail is chosen automatically:

1. **Wire** if amount > $25,000
2. **FedNow** if the bank supports it
3. **RTP** if the bank supports it
4. **ACH** (default fallback)

---

## Payment Flow

```
1. Create Session ──▶ POST /api/sessions
                        │
2. Customer Pays   ──▶ GET /pay/:id
                        │
3. Link Bank (opt) ──▶ Plaid Link Token → Exchange → Auth
                        │
4. Confirm Payment ──▶ POST /api/sessions/:id/confirm
                        │   Creates counterparty (MT or Column)
                        │
5. Initiate        ──▶ POST /api/sessions/:id/initiate
                        │   Creates payment order / transfer
                        │
6. Webhook         ◀── Column/MT sends status update
                        │   Session marked completed/expired
                        │
7. SSE Stream      ──▶ GET /api/sessions/:id/stream
                        Real-time status updates via Server-Sent Events
```

### Step Details

1. **Create Session**: Choose a bank, enter amount. Rail is auto-selected. Session expires in 24h.
2. **Pay Page**: Customer sees QR code, amount, and bank linking section. Live status updates via SSE.
3. **Plaid Link**: Customer links their bank account via Plaid to retrieve account/routing numbers.
4. **Confirm**: Updates customer details and creates a counterparty (receiving account) at MT or Column.
5. **Initiate**: Sends the actual payment order/transfer. Session moves to `processing`.
6. **Webhook**: MT or Column sends a webhook when the transfer settles/fails. Session updates accordingly.
7. **SSE**: The pay page receives real-time updates without polling.

---

## Admin Dashboard

The admin SPA at `/admin/` is built with **Refine v4** + **Ant Design 5** + **recharts**.

### Pages

| Page | Path | Description |
|---|---|---|
| **Dashboard** | `/admin/` | KPI cards (today total, completed, pending, volume), bar chart by rail, 7-day line chart |
| **Sessions** | `/admin/sessions` | Filterable table with status/date filters, expandable rows |
| **Merchants** | `/admin/merchants` | CRUD: list, create, edit, show with KYC status |
| **Banks** | `/admin/banks` | Read-only list with rail support indicators |
| **Settlements** | `/admin/settlements` | Aggregated per-merchant settlement data |
| **Webhooks** | `/admin/webhooks` | Webhook event log with status tracking |

### Data Provider

Custom `@refinedev/simple-rest` wrapper at `/api/admin` with pagination (`limit`/`offset`) and filter support via query parameters.

---

## Prerequisites

- **Rust** 1.75+ (`rustup`, `cargo`)
- **Node.js** 20+ and **npm**
- **PostgreSQL** 16 (or Docker)
- **Docker** (optional, for containerized setup)

---

## Setup

### 1. Clone and Environment

```bash
git clone git@github.com:omarharoun/NoorPay.git
cd paybank-monolith
cp .env.example .env
```

Edit `.env` with your credentials (see [Environment Variables](#environment-variables)).

### 2. Database

**Option A: Docker**

```bash
docker run -d \
  --name paybank-postgres \
  -e POSTGRES_USER=noralpay \
  -e POSTGRES_PASSWORD=noralpay_dev_password \
  -e POSTGRES_DB=noralpay \
  -p 5433:5432 \
  postgres:16-alpine
```

**Option B: Local PostgreSQL**

Create a database named `noralpay` and run:

```bash
sqlx migrate run
# or
psql postgresql://noralpay:noralpay_dev_password@localhost:5433/noralpay -f migrations/001_initial_schema.sql
psql postgresql://noralpay:noralpay_dev_password@localhost:5433/noralpay -f migrations/002_seed_data.sql
```

### 3. Build and Run the API

```bash
# Build
cargo build --release

# Run
./target/release/paybank
```

The API starts on `0.0.0.0:8888`.

### 4. Admin Dashboard

```bash
cd admin

# Install dependencies
npm install

# Development (with HMR at :5173)
npm run dev

# Production build
npm run build
```

In development, the Vite dev server proxies `/api` requests to `localhost:8888`. In production, the Rust server serves the built files from `admin/dist/`.

### 5. Docker (Full Stack)

```bash
docker compose up -d
```

This starts PostgreSQL and the API with the pre-built admin dashboard.

---

## Development

```bash
# Terminal 1: Rust API (auto-reload with cargo-watch)
cargo watch -x run

# Terminal 2: Admin dashboard (HMR at :5173)
cd admin && npm run dev
```

### Running Tests

```bash
# Rust tests (requires running database)
cargo test

# Admin lint
cd admin && npm run lint
```

### Checking Types

```bash
# Rust
cargo check

# Admin TypeScript
cd admin && npx tsc --noEmit
```

---

## Deployment

### Docker Build

```bash
docker compose build
docker compose up -d
```

The multi-stage Dockerfile:
1. Builds the React admin (`node:22-alpine`)
2. Builds the Rust binary (`rust:1-slim-bookworm`)
3. Produces a slim runtime image (`debian:bookworm-slim`, ~50MB)

### Environment Variables (Production)

Ensure all env vars in `.env` are set for your production environment:

| Variable | Example | Required For |
|---|---|---|
| `DATABASE_URL` | `postgresql://user:pass@host:5432/db` | Startup |
| `BIND_ADDR` | `0.0.0.0:8888` | Startup |
| `PUBLIC_APP_URL` | `https://noorpay.com` | Payment links, QR codes |
| `MODERN_TREASURY_ORG_ID` | `uuid` | Modern Treasury |
| `MODERN_TREASURY_API_KEY` | `sk_xxxxx` | Modern Treasury |
| `MT_INTERNAL_ACCOUNT_ID` | `uuid` | Modern Treasury |
| `COLUMN_API_KEY` | `col_sandbox_xxxxx` | Column |
| `COLUMN_MERCHANT_ACCOUNT_ID` | `sandbox-account-id` | Column |
| `PLAID_CLIENT_ID` | from Plaid dashboard | Plaid |
| `PLAID_SECRET` | from Plaid dashboard | Plaid |
| `WEBHOOK_SECRET` | 32-char random string | Column webhook verification |

---

## Project Structure

```
paybank-monolith/
├── Cargo.toml              # Rust workspace root
├── src/main.rs             # Server entrypoint
├── crates/
│   ├── paybank-core/       # Domain types, errors, models
│   ├── paybank-db/         # Database repos (admin, session, bank, etc.)
│   ├── paybank-api/        # Axum routes, handlers, state
│   ├── paybank-payments/   # MT, Column, Plaid integrations
│   └── paybank-qr/         # QR code generation
├── migrations/             # SQLx migrations
├── admin/                  # React + Refine admin dashboard
│   ├── src/pages/          # Dashboard, Sessions, Merchants, etc.
│   └── dist/               # Production build output
├── web/                    # Static HTML pages (pay, invoice, dashboard)
├── Dockerfile              # Multi-stage build
├── docker-compose.yml      # Full stack deployment
└── .env.example            # Environment variable template
```
