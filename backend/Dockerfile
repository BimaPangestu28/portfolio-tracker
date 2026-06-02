# syntax=docker/dockerfile:1

# ---- Build stage ----
FROM rust:1-bookworm AS builder
WORKDIR /app

# Compile dependencies first (cached unless Cargo.toml/lock change) by building
# a throwaway empty binary, then rebuild with the real source.
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo 'fn main() {}' > src/main.rs \
 && cargo build --release --locked \
 && rm -rf src

COPY . .
# Embedded migrations (sqlx::migrate!) require ./migrations to be present here.
RUN touch src/main.rs && cargo build --release --locked

# ---- Runtime stage ----
FROM debian:bookworm-slim
# ca-certificates: TLS roots for outbound calls (Anthropic API, price feeds).
# curl: used by the compose healthcheck.
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl \
 && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/portfolio-tracker /usr/local/bin/portfolio-tracker

# Bind on all interfaces so other containers can reach it; store the SQLite DB
# on the mounted volume so data survives container recreation.
ENV BIND_ADDR=0.0.0.0:8080 \
    DATABASE_URL=sqlite:///data/portfolio.db

EXPOSE 8080
VOLUME ["/data"]
CMD ["portfolio-tracker"]
