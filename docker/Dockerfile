FROM debian:bookworm AS builder

# Install Rust and build dependencies
RUN apt-get update && apt-get install -y \
    curl \
    gcc \
    pkg-config \
    libssl-dev \
    libpq-dev \
    && curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y \
    && rm -rf /var/lib/apt/lists/*

ENV PATH="/root/.cargo/bin:${PATH}"
WORKDIR /app

# Copy source code
COPY . .

# Build the application
RUN cargo build --release

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    curl \
    libpq5 \
    && rm -rf /var/lib/apt/lists/*

# Create app user
RUN useradd -m -u 1000 appuser

# Copy the binary from builder stage
COPY --from=builder /app/target/release/matrix-bridge-discord /usr/local/bin/

# Create data directory
RUN mkdir -p /data && chown appuser:appuser /data

# Switch to non-root user
USER appuser

# Expose port
EXPOSE 9005

# Set working directory
WORKDIR /data

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:9005/health || exit 1

# Run the application
CMD ["matrix-bridge-discord"]