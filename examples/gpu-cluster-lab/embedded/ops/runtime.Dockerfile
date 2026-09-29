FROM node:24-bookworm-slim AS ui
ARG NPM_REGISTRY=https://registry.npmjs.org/
ENV npm_config_registry=$NPM_REGISTRY
WORKDIR /ui
COPY embedded/.build/npm-cache/ /npm-cache/
COPY embedded/.build/runtime-src/drasi-server/ui/package.json embedded/.build/runtime-src/drasi-server/ui/package-lock.json ./
RUN --mount=type=cache,id=gpu-lab-npm,target=/root/.npm \
    set -- /npm-cache/*.tgz; \
    if [ -f "$1" ]; then npm cache add --offline --ignore-scripts "$@"; fi; \
    npm ci --no-audit --no-fund
COPY embedded/.build/runtime-src/drasi-server/ui/ ./
RUN npm run build

FROM rust:1.95-bookworm AS build
ARG TARGETARCH
RUN apt-get update -qq && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev libpq-dev libjq-dev libonig-dev protobuf-compiler \
    libprotobuf-dev cmake clang libclang-dev \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /workspace
COPY embedded/.build/runtime-src/ ./
COPY --from=ui /ui/dist/ /workspace/drasi-server/ui/dist/
RUN mkdir -p /workspace/licenses && \
    if [ -f /workspace/drasi-core/core/src/hashing/spooky.rs ]; then \
      cp /workspace/drasi-core/core/src/hashing/LICENSE-MIT /workspace/licenses/SpookyHash-LICENSE-MIT; \
    fi
WORKDIR /workspace/drasi-server/examples/gpu-cluster-lab/shared/crates/native
RUN --mount=type=cache,id=gpu-lab-registry,target=/usr/local/cargo/registry \
    --mount=type=cache,id=gpu-lab-native-linux-${TARGETARCH},target=/workspace/drasi-server/examples/gpu-cluster-lab/shared/target/linux \
    export CARGO_BUILD_JOBS=1 \
      CARGO_TARGET_DIR=/workspace/drasi-server/examples/gpu-cluster-lab/shared/target/linux \
      JQ_LIB_DIR=/usr/lib/$(gcc -dumpmachine) && \
    cargo build --locked --release --features dynamic-plugin --lib && \
    cp "$CARGO_TARGET_DIR/release/libgpu_native.so" /workspace/libgpu_native.so && \
    cargo build --locked --release --features runtime --bin gpu-runtime && \
    cp "$CARGO_TARGET_DIR/release/gpu-runtime" /workspace/gpu-runtime

FROM debian:bookworm-slim
RUN apt-get update -qq && apt-get install -y --no-install-recommends \
    ca-certificates libssl3 libpq5 libjq1 libonig5 curl \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=build /workspace/gpu-runtime /app/gpu-runtime
COPY --from=build /workspace/libgpu_native.so /app/libgpu_native.so
COPY --from=build /workspace/source-manifest.json /app/source-manifest.json
COPY --from=build /workspace/licenses/ /app/licenses/
ENV GPU_NATIVE_PLUGIN=/app/libgpu_native.so
USER 65532:65532
CMD ["/app/gpu-runtime"]
