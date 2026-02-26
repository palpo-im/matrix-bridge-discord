FROM rust:1.93 AS builder

RUN apt-get update && apt-get install -y musl-dev musl-tools && \
    rustup target add x86_64-unknown-linux-musl

WORKDIR /workspace
COPY Cargo.toml Cargo.lock ./
COPY src/ ./src/
RUN cargo build --release -p matrix-bridge-discord --target x86_64-unknown-linux-musl

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    libpq5 \
    && rm -rf /var/lib/apt/lists/*

RUN useradd -m -u 1000 appuser

COPY --from=builder /workspace/target/x86_64-unknown-linux-musl/release/matrix-bridge-discord /usr/local/bin/matrix-bridge-discord

USER appuser
WORKDIR /data

EXPOSE 9005

HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:9005/health || exit 1

CMD ["matrix-bridge-discord"]
