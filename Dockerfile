# =============================================================================
# nanobot Dockerfile - Multi-platform (Rust + Python)
# =============================================================================
# Build target: rust (default) or python
# Usage:
#   docker build -t nanobot .
#   docker build -t nanobot-python --target python .
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
COPY nanobot-rs/Cargo.toml nanobot-rs/Cargo.lock ./
COPY nanobot-rs/nanobot-core/Cargo.toml ./nanobot-core/
COPY nanobot-rs/nanobot-cli/Cargo.toml ./nanobot-cli/

# Create dummy files to build dependencies
RUN mkdir -p nanobot-core/src nanobot-cli/src && \
    echo "pub fn dummy() {}" > nanobot-core/src/lib.rs && \
    echo "fn main() {}" > nanobot-cli/src/main.rs && \
    cargo build --release --features all-channels && \
    rm -rf nanobot-core/src nanobot-cli/src

# Copy actual source and build
COPY nanobot-rs/nanobot-core/src ./nanobot-core/src
COPY nanobot-rs/nanobot-cli/src ./nanobot-cli/src

RUN touch nanobot-core/src/lib.rs nanobot-cli/src/main.rs && \
    cargo build --release --features all-channels

# -----------------------------------------------------------------------------
# Stage 2: Python Builder
# -----------------------------------------------------------------------------
FROM ghcr.io/astral-sh/uv:python3.12-bookworm-slim AS python-builder

# Install Node.js 20 for the WhatsApp bridge
RUN apt-get update && \
    apt-get install -y --no-install-recommends curl ca-certificates gnupg git && \
    mkdir -p /etc/apt/keyrings && \
    curl -fsSL https://deb.nodesource.com/gpgkey/nodesource-repo.gpg.key | gpg --dearmor -o /etc/apt/keyrings/nodesource.gpg && \
    echo "deb [signed-by=/etc/apt/keyrings/nodesource.gpg] https://deb.nodesource.com/node_20.x nodistro main" > /etc/apt/sources.list.d/nodesource.list && \
    apt-get update && \
    apt-get install -y --no-install-recommends nodejs && \
    apt-get purge -y gnupg && \
    apt-get autoremove -y && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Install Python dependencies first (cached layer)
COPY pyproject.toml README.md LICENSE ./
RUN mkdir -p nanobot bridge && touch nanobot/__init__.py && \
    uv pip install --system --no-cache . && \
    rm -rf nanobot bridge

# Copy the full source and install
COPY nanobot/ nanobot/
COPY bridge/ bridge/
RUN uv pip install --system --no-cache .

# Build the WhatsApp bridge
WORKDIR /app/bridge
RUN npm install && npm run build
WORKDIR /app

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
COPY --from=rust-builder /build/target/release/nanobot /usr/local/bin/nanobot

# Create config directory
RUN mkdir -p /root/.nanobot

# Gateway default port
EXPOSE 18790

ENTRYPOINT ["nanobot"]
CMD ["status"]

# -----------------------------------------------------------------------------
# Stage 4: Python Runtime
# -----------------------------------------------------------------------------
FROM python-builder AS python

WORKDIR /app

# Create config directory
RUN mkdir -p /root/.nanobot

# Gateway default port
EXPOSE 18790

ENTRYPOINT ["nanobot"]
CMD ["status"]