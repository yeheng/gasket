# =============================================================================
# gasket Dockerfile - Multi-platform (Rust)
# =============================================================================
# Build target: rust (default)
# Usage:
#   docker build -t gasket .
# =============================================================================

# -----------------------------------------------------------------------------
# Stage 1: Rust Builder
# -----------------------------------------------------------------------------
FROM rust:1.75-bookworm AS rust-builder

WORKDIR /build

# Install dependencies
RUN apt-get update && \
    apt-get install -y --no-install-recommends pkg-config libssl-dev && \
    rm -rf /var/lib/apt/lists/*

# Copy Cargo files first for caching
COPY gasket/Cargo.toml gasket/Cargo.lock ./
COPY gasket/gasket-core/Cargo.toml ./gasket-core/
COPY gasket/gasket-cli/Cargo.toml ./gasket-cli/

# Create dummy files to build dependencies
RUN mkdir -p gasket-core/src gasket-cli/src && \
    echo "pub fn dummy() {}" > gasket-core/src/lib.rs && \
    echo "fn main() {}" > gasket-cli/src/main.rs && \
    cargo build --release --features all-channels && \
    rm -rf gasket-core/src gasket-cli/src

# Copy actual source and build
COPY gasket/gasket-core/src ./gasket-core/src
COPY gasket/gasket-cli/src ./gasket-cli/src

RUN touch gasket-core/src/lib.rs gasket-cli/src/main.rs && \
    cargo build --release --features all-channels

# -----------------------------------------------------------------------------
# Stage 3: Rust Runtime (Default)
# -----------------------------------------------------------------------------
FROM debian:bookworm-slim AS rust

# Install runtime dependencies
RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates libssl3 && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy binary from builder
COPY --from=rust-builder /build/target/release/gasket /usr/local/bin/gasket

# Create config directory
RUN mkdir -p /root/.gasket

# Gateway default port
EXPOSE 18790

ENTRYPOINT ["gasket"]
CMD ["status"]
