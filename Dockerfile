FROM node:22-alpine AS node-builder
WORKDIR /admin
COPY admin/package.json admin/package-lock.json* ./
RUN npm ci
COPY admin/ .
RUN npm run build

FROM rust:1-slim-bookworm AS builder
WORKDIR /app
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
COPY . .
RUN SQLX_OFFLINE=true cargo build --release --bin paybank

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/paybank /usr/local/bin/paybank
COPY --from=builder /app/web /web
COPY --from=node-builder /admin/dist /admin/dist
EXPOSE 8888
CMD ["paybank"]
