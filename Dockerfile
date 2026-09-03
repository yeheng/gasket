# =============================================================================
# conga Dockerfile - Multi-stage build (Rust gateway)
# =============================================================================
# Produces a single image running `conga-gateway` on port 3000. The gateway
# serves only /ws + /api/* — deploy the web frontend separately (any static
# host) and point it at this gateway via VITE_WS_URL / VITE_API_URL.
#
# Usage:
#   docker build -t conga .
#   docker run -d -p 3000:3000 \
#     -e CONGA_LLM_BASE_URL=... -e CONGA_LLM_KEY=... \
#     -e CONGA_LLM_MODEL=... -e CONGA_LLM_API=openai \
#     conga
# =============================================================================

# -----------------------------------------------------------------------------
# Stage 1: Rust builder (conga-gateway binary)
# -----------------------------------------------------------------------------
FROM rust:1.82-bookworm AS rust-builder

WORKDIR /build

# Copy workspace root files for dependency caching
COPY conga/Cargo.toml conga/Cargo.lock ./

# Copy all workspace member Cargo.toml files
COPY conga/conga/Cargo.toml ./conga/
COPY conga/conga-host/Cargo.toml ./conga-host/
COPY conga/conga-cli/Cargo.toml ./conga-cli/
COPY conga/conga-ext/Cargo.toml ./conga-ext/
COPY conga/conga-gateway/Cargo.toml ./conga-gateway/

# Create dummy source files so cargo can build dependencies layer
RUN mkdir -p \
        conga/src \
        conga-host/src \
        conga-cli/src \
        conga-ext/src \
        conga-gateway/src && \
    echo "pub fn dummy() {}" > conga/src/lib.rs && \
    echo "pub fn dummy() {}" > conga-host/src/lib.rs && \
    echo "fn main() {}" > conga-cli/src/main.rs && \
    echo "pub fn dummy() {}" > conga-ext/src/lib.rs && \
    echo "fn main() {}" > conga-gateway/src/main.rs && \
    cargo build --release --bin conga-gateway --all-features && \
    rm -rf \
        conga/src \
        conga-host/src \
        conga-cli/src \
        conga-ext/src \
        conga-gateway/src

# Copy actual source code
COPY conga/conga/src ./conga/src
COPY conga/conga-host/src ./conga-host/src
COPY conga/conga-cli/src ./conga-cli/src
COPY conga/conga-ext/src ./conga-ext/src
COPY conga/conga-gateway/src ./conga-gateway/src

# Touch source files to invalidate cargo cache and rebuild
RUN touch \
        conga/src/lib.rs \
        conga-host/src/lib.rs \
        conga-cli/src/main.rs \
        conga-ext/src/lib.rs \
        conga-gateway/src/main.rs && \
    cargo build --release --bin conga-gateway --all-features

# -----------------------------------------------------------------------------
# Stage 2: Runtime
# -----------------------------------------------------------------------------
FROM debian:bookworm-slim AS runtime

# Install runtime dependencies
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
        ca-certificates \
        libssl3 && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the gateway binary
COPY --from=rust-builder /build/target/release/conga-gateway /usr/local/bin/conga-gateway

# Create config directory
RUN mkdir -p /root/.conga

# Gateway default port
EXPOSE 3000

# The gateway binds 127.0.0.1 by default (it runs the agent's bash tool).
# Inside the container the port is only reachable via an explicit -p, so the
# container network — not the gateway — is the boundary here.
ENV CONGA_GATEWAY_HOST=0.0.0.0

ENTRYPOINT ["conga-gateway"]
CMD []
