FROM rust:1.95-bookworm AS build
ARG TARGETARCH
RUN apt-get update -qq && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev libpq-dev libjq-dev libonig-dev protobuf-compiler \
    libprotobuf-dev cmake clang libclang-dev \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /workspace
COPY embedded/.build/runtime-src/ ./
RUN mkdir -p /workspace/licenses && \
    if [ -f /workspace/drasi-core/core/src/hashing/spooky.rs ]; then \
      cp /workspace/drasi-core/core/src/hashing/LICENSE-MIT /workspace/licenses/SpookyHash-LICENSE-MIT; \
    fi
# Overlay only the diagnostic and shared unchanged query definitions, not newer Core sources.
COPY shared/crates/native/src/bin/query-isolation.rs drasi-server/examples/gpu-cluster-lab/shared/crates/native/src/bin/query-isolation.rs
COPY shared/crates/native/src/inputs.rs drasi-server/examples/gpu-cluster-lab/shared/crates/native/src/inputs.rs
COPY shared/crates/native/Cargo.toml drasi-server/examples/gpu-cluster-lab/shared/crates/native/Cargo.toml
COPY shared/crates/native/Cargo.lock drasi-server/examples/gpu-cluster-lab/shared/crates/native/Cargo.lock
WORKDIR /workspace/drasi-server/examples/gpu-cluster-lab/shared/crates/native
RUN sha256sum Cargo.toml Cargo.lock src/inputs.rs src/bin/query-isolation.rs > /workspace/isolation-manifest.sha256
RUN --mount=type=cache,id=gpu-lab-registry,target=/usr/local/cargo/registry \
    --mount=type=cache,id=gpu-lab-native-linux-${TARGETARCH},target=/workspace/drasi-server/examples/gpu-cluster-lab/shared/target/linux \
    export CARGO_BUILD_JOBS=1 CARGO_TARGET_DIR=/workspace/drasi-server/examples/gpu-cluster-lab/shared/target/linux \
      JQ_LIB_DIR=/usr/lib/$(gcc -dumpmachine) && \
    cargo build --locked --release --features query-isolation --bin query-isolation && \
    cargo test --locked --release --features query-isolation --bin query-isolation && \
    cp "$CARGO_TARGET_DIR/release/query-isolation" /workspace/query-isolation

FROM build AS validate
RUN rustup component add clippy
COPY embedded/src/ /workspace/drasi-server/examples/gpu-cluster-lab/embedded/src/
RUN --mount=type=cache,id=gpu-lab-registry,target=/usr/local/cargo/registry \
    --mount=type=cache,id=gpu-lab-native-linux-${TARGETARCH},target=/workspace/drasi-server/examples/gpu-cluster-lab/shared/target/linux \
    export CARGO_BUILD_JOBS=1 CARGO_TARGET_DIR=/workspace/drasi-server/examples/gpu-cluster-lab/shared/target/linux \
      JQ_LIB_DIR=/usr/lib/$(gcc -dumpmachine) && \
    cargo clippy --locked --release --features query-isolation,runtime --all-targets -- -D warnings && \
    cargo test --locked --release --features query-isolation,runtime --lib --bins

FROM debian:bookworm-slim
RUN apt-get update -qq && apt-get install -y --no-install-recommends \
    ca-certificates libssl3 libpq5 libjq1 libonig5 \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /workspace/query-isolation /app/query-isolation
COPY --from=build /workspace/source-manifest.json /app/source-manifest.json
COPY --from=build /workspace/isolation-manifest.sha256 /app/isolation-manifest.sha256
COPY --from=build /workspace/licenses/ /app/licenses/
USER 65532:65532
ENTRYPOINT ["/app/query-isolation"]
