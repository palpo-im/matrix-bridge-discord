FROM rust:1.75 AS builder

WORKDIR /workspace
COPY Cargo.toml Cargo.lock ./
COPY src/ ./src/
RUN cargo build --release -p matrix-bridge-discord

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

RUN useradd -m -u 1000 appuser

COPY --from=builder /workspace/target/release/matrix-bridge-discord /usr/local/bin/matrix-bridge-discord

USER appuser
WORKDIR /data

EXPOSE 9005

HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:9005/health || exit 1

CMD ["matrix-bridge-discord"]
