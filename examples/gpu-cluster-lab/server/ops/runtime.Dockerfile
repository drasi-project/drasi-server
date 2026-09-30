FROM node:24-bookworm-slim AS ui
ARG NPM_REGISTRY=https://registry.npmjs.org/
ENV npm_config_registry=$NPM_REGISTRY
WORKDIR /ui
COPY server/.build/npm-cache/ /npm-cache/
COPY server/.build/runtime-src/drasi-server/ui/package.json server/.build/runtime-src/drasi-server/ui/package-lock.json ./
RUN --mount=type=cache,id=gpu-lab-npm,target=/root/.npm \
    set -- /npm-cache/*.tgz; \
    if [ -f "$1" ]; then npm cache add --offline --ignore-scripts "$@"; fi; \
    npm ci --no-audit --no-fund
COPY server/.build/runtime-src/drasi-server/ui/ ./
RUN npm run build

FROM rust:1.95-bookworm AS build
ARG TARGETARCH
RUN apt-get update -qq && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev libpq-dev libjq-dev libonig-dev protobuf-compiler \
    libprotobuf-dev cmake clang libclang-dev \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /workspace
COPY server/.build/runtime-src/ ./
COPY --from=ui /ui/dist/ /workspace/drasi-server/ui/dist/
RUN mkdir -p /workspace/artifacts/plugins /workspace/artifacts/licenses && \
    cp /workspace/drasi-core/core/src/hashing/LICENSE-MIT \
       /workspace/artifacts/licenses/SpookyHash-LICENSE-MIT
RUN --mount=type=cache,id=gpu-lab-registry,target=/usr/local/cargo/registry \
    --mount=type=cache,id=gpu-lab-server-linux-${TARGETARCH},target=/target \
    export CARGO_BUILD_JOBS=1 CARGO_TARGET_DIR=/target \
      JQ_LIB_DIR=/usr/lib/$(gcc -dumpmachine) && \
    cargo build --manifest-path drasi-server/Cargo.toml --locked --release --bin drasi-server && \
    cp /target/release/drasi-server artifacts/ && \
    cargo build --manifest-path drasi-core/Cargo.toml --locked --release \
      -p drasi-source-postgres -p drasi-bootstrap-postgres -p drasi-reaction-sse \
      --features drasi-source-postgres/dynamic-plugin,drasi-bootstrap-postgres/dynamic-plugin,drasi-reaction-sse/dynamic-plugin && \
    cp /target/release/libdrasi_source_postgres.so \
       /target/release/libdrasi_bootstrap_postgres.so \
       /target/release/libdrasi_reaction_sse.so artifacts/plugins/ && \
    cargo build --manifest-path drasi-server/examples/gpu-cluster-lab/shared/crates/native/Cargo.toml \
      --locked --release --features dynamic-plugin --lib && \
    cp /target/release/libgpu_native.so artifacts/plugins/libdrasi_gpu_lab.so && \
    cargo test --manifest-path drasi-server/examples/gpu-cluster-lab/shared/crates/native/Cargo.toml \
      --locked --release --features dynamic-plugin --lib && \
    cargo test --manifest-path drasi-server/examples/gpu-cluster-lab/server/Cargo.toml \
      --locked --release && \
    cd artifacts && sha256sum drasi-server plugins/*.so > artifacts.sha256

FROM debian:bookworm-slim
RUN apt-get update -qq && apt-get install -y --no-install-recommends \
    ca-certificates libssl3 libpq5 libjq1 libonig5 curl procps \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=build /workspace/artifacts/ /app/
COPY --from=build /workspace/source-manifest.json /app/source-manifest.json
USER 65532:65532
ENTRYPOINT ["/app/drasi-server"]
CMD ["--config", "/app/server.yaml", "--plugins-dir", "/app/plugins", "--skip-verification"]
