# Copyright 2025 The Drasi Authors.
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#     http:#www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

# Drasi Server Dockerfile
# Multi-stage build for minimal runtime image
#
# Build:
#   docker build -t drasi-server .
#
# Run:
#   docker run -p 8080:8080 -v ./config:/app/config drasi-server

# =============================================================================
# Stage 1: Build Environment
# =============================================================================
FROM rust:1.88-slim-bookworm AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    pkg-config \
    libssl-dev \
    libpq-dev \
    libjq-dev \
    libonig-dev \
    protobuf-compiler \
    libprotobuf-dev \
    cmake \
    git \
    clang \
    libclang-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy Cargo files first for dependency caching
COPY Cargo.toml Cargo.lock ./


# Copy source code
COPY src ./src
COPY config ./config

# Build release binary
# Set JQ_LIB_DIR dynamically for multiarch support (no pkg-config file in Debian's libjq-dev)
RUN JQ_LIB_DIR=$(dirname $(find /usr/lib -name 'libjq.so' | head -1)) \
    cargo build --release --bin drasi-server

# =============================================================================
# Stage 2: Runtime Environment
# =============================================================================
FROM debian:bookworm-slim AS runtime

# Install runtime dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    libpq5 \
    libjq1 \
    libonig5 \
    curl \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd -r drasi && useradd -r -g drasi drasi

WORKDIR /app

# Copy binary from builder
COPY --from=builder /app/target/release/drasi-server /usr/local/bin/drasi-server

# Create config directory with proper permissions
RUN mkdir -p /app/config && chown -R drasi:drasi /app

# Copy default config (will be overridden by volume mount)
COPY --chown=drasi:drasi config/server-docker.yaml /app/config/server.yaml

# Switch to non-root user
USER drasi

# Expose default API port
EXPOSE 8080

# Health check using /health endpoint
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -f http://localhost:8080/health || exit 1

# Default entrypoint
ENTRYPOINT ["drasi-server"]
CMD ["--config", "/app/config/server.yaml"]
