# Multi-stage Dockerfile for Gnapsis
# Supports both AMD64 and ARM64 (Raspberry Pi)

# ============================================
# Stage 1: Builder
# ============================================
FROM rust:1.91-slim-trixie AS builder

WORKDIR /app

# Install build dependencies (including Wayland for Bevy, C++ for native libs)
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    libwayland-dev \
    libxkbcommon-dev \
    g++ \
    && rm -rf /var/lib/apt/lists/*

# Copy manifests first for better caching
COPY Cargo.toml Cargo.lock ./
COPY di-macros/Cargo.toml di-macros/Cargo.toml

# Create dummy files for dependency caching
RUN mkdir -p src di-macros/src && \
    echo "fn main() {}" > src/main.rs && \
    echo "" > di-macros/src/lib.rs

# Build dependencies only (cached layer)
RUN cargo build --release && rm -rf src di-macros/src

# Copy actual source code
COPY src ./src
COPY di-macros/src ./di-macros/src

# Touch source files to invalidate cache and rebuild (including di-macros for proc macros)
RUN touch src/main.rs di-macros/src/lib.rs && cargo build --release

# ============================================
# Stage 2: Embedding Model Warmup
# ============================================
FROM builder AS warmup

# Create config for embedding warmup
RUN mkdir -p /app/.config
RUN echo '[embedding]\n\
model = "BAAI/bge-small-en-v1.5"\n\
\n\
[neo4j]\n\
uri = "bolt://localhost:7687"\n\
user = "neo4j"\n\
' > /app/.gnapsis.toml

# Pre-download embedding model
ENV FASTEMBED_CACHE_DIR=/app/.fastembed_cache
RUN ./target/release/gnapsis embedding warmup

# ============================================
# Stage 3: Runtime
# ============================================
FROM debian:trixie-slim AS runtime

# Install runtime dependencies (including Wayland for Bevy)
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    libwayland-client0 \
    libxkbcommon0 \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -m -u 1000 gnapsis
USER gnapsis
WORKDIR /home/gnapsis

# Copy binary and embedding model cache
COPY --from=warmup --chown=gnapsis:gnapsis /app/target/release/gnapsis /usr/local/bin/
COPY --from=warmup --chown=gnapsis:gnapsis /app/.fastembed_cache /home/gnapsis/.fastembed_cache

# Set environment
ENV FASTEMBED_CACHE_DIR=/home/gnapsis/.fastembed_cache

# Expose HTTP port
EXPOSE 3000

# Default command
CMD ["gnapsis", "serve", "--host", "0.0.0.0", "--port", "3000"]
