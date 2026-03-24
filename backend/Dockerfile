# ── Build stage ───────────────────────────────────────────────────────────────
FROM rust:1.85-slim-bookworm AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    libsqlite3-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Fetch dependencies first so this layer is cached between source changes.
# A stub src/main.rs is required because cargo needs at least one target to
# parse the manifest — it gets replaced by the real source in the next step.
COPY Cargo.toml Cargo.lock ./
RUN mkdir -p src && echo "fn main() {}" > src/main.rs && cargo fetch && rm -rf src

# Build the actual source
COPY src ./src
RUN cargo build --release

# ── Runtime stage ─────────────────────────────────────────────────────────────
FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
    libssl3 \
    libsqlite3-0 \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /app/target/release/oddsforge-backend .

# /data is mounted as a Fly.io persistent volume
RUN mkdir -p /data

ENV DATABASE_URL=sqlite:///data/oddsforge.db
ENV RUST_LOG=info

EXPOSE 3000
CMD ["./oddsforge-backend"]
