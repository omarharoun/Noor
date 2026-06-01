# ── Stage 1: build admin ──────────────────────────────────────────────────
FROM node:20-alpine AS node-builder
WORKDIR /admin
COPY admin/package.json admin/package-lock.json* ./
RUN npm ci
COPY admin/ .
RUN npm run build

# ── Stage 2: build Rust API ───────────────────────────────────────────────
FROM rust:1-slim-bookworm AS rust-builder
WORKDIR /app

# Install system deps required by sqlx / openssl
RUN apt-get update \
    && apt-get install -y --no-install-recommends pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy workspace manifests first for better layer caching
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
COPY src/ src/

# sqlx::migrate!("./migrations") embeds the migration files at COMPILE time, so
# the directory must exist in the build stage too — not only the runtime stage.
COPY migrations/ migrations/

# Copy the sqlx offline query cache so we can build without a live DB
COPY .sqlx/ .sqlx/

# Build in release mode using the offline cache
RUN SQLX_OFFLINE=true cargo build --release --bin paybank

# ── Stage 3: slim runtime image ───────────────────────────────────────────
FROM debian:bookworm-slim AS runtime

# Install curl (for HEALTHCHECK) and CA certificates; then clean up
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*

# Create a non-root user to run the service
RUN groupadd --gid 10001 app \
    && useradd --uid 10001 --gid app --shell /bin/false --no-create-home app

WORKDIR /app

# Copy artefacts from builder stages
COPY --from=rust-builder /app/target/release/paybank /usr/local/bin/paybank
COPY --from=node-builder /admin/dist /app/admin/dist
COPY web/ /app/web/
COPY migrations/ /app/migrations/

# Switch to non-root user
USER app

EXPOSE 8888

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -f http://localhost:8888/health || exit 1

CMD ["paybank"]
