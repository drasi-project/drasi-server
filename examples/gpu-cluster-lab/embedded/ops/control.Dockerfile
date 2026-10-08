FROM node:24-bookworm-slim AS ui
ARG NPM_REGISTRY=https://registry.npmjs.org/
ENV npm_config_registry=$NPM_REGISTRY
WORKDIR /react-package
COPY embedded/.build/npm-cache/ /npm-cache/
COPY embedded/.build/react-source/dev-tools/react/ ./
RUN --mount=type=cache,id=gpu-lab-npm,target=/root/.npm \
    set -- /npm-cache/*.tgz; \
    if [ -f "$1" ]; then npm cache add --offline --ignore-scripts "$@"; fi; \
    npm ci --ignore-scripts --no-audit --no-fund && npm run build && npm pack --ignore-scripts --pack-destination /tmp
WORKDIR /ui
COPY embedded/.build/runtime-src/drasi-server/examples/native-sse-client/ /native-sse-client/
COPY shared/ui/package.json shared/ui/package-lock.json shared/ui/.npmrc ./
RUN --mount=type=cache,id=gpu-lab-npm,target=/root/.npm \
    mkdir vendor && cp /tmp/drasi-react-0.1.0.tgz vendor/drasi-react.tgz && npm ci --no-audit --no-fund
COPY shared/ui/ ./
RUN npm run build

FROM rust:1.95-bookworm AS rust
ARG TARGETARCH
ARG CARGO_BUILD_JOBS=1
ENV CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS} CARGO_INCREMENTAL=0
WORKDIR /app
COPY rust-toolchain.toml ./
COPY shared/Cargo.toml shared/Cargo.lock shared/
COPY shared/crates/ shared/crates/
COPY shared/policies/ shared/policies/
COPY shared/migrations/ shared/migrations/
COPY embedded/control/ embedded/control/
WORKDIR /app/shared
RUN --mount=type=cache,id=gpu-lab-registry,target=/usr/local/cargo/registry \
    --mount=type=cache,id=gpu-lab-control-linux-${TARGETARCH},target=/app/target \
    cargo build --locked --release --target-dir /app/target -p gpu-control && cp /app/target/release/gpu-control /app/gpu-control

FROM debian:bookworm-slim
RUN apt-get update -qq && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=rust /app/gpu-control target/release/gpu-control
COPY --from=ui /ui/dist ui/dist
USER 65532:65532
CMD ["./target/release/gpu-control"]
