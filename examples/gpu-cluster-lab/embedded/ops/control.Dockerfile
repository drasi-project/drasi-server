FROM node:24-bookworm-slim AS ui
ARG NPM_REGISTRY=https://registry.npmjs.org/
ARG HOSTING=embedded
ARG SOURCE_ROOT=.
ENV npm_config_registry=$NPM_REGISTRY
WORKDIR /react-package
COPY ${HOSTING}/.build/npm-cache/ /npm-cache/
COPY ${HOSTING}/.build/react-source/dev-tools/react/ ./
RUN --mount=type=cache,id=gpu-lab-npm,target=/root/.npm \
    set -- /npm-cache/*.tgz; \
    if [ -f "$1" ]; then npm cache add --offline --ignore-scripts "$@"; fi; \
    npm ci --ignore-scripts --no-audit --no-fund && npm run build && npm pack --ignore-scripts --pack-destination /tmp
WORKDIR /ui
COPY ${SOURCE_ROOT}/shared/ui/package.json ${SOURCE_ROOT}/shared/ui/package-lock.json ./
RUN --mount=type=cache,id=gpu-lab-npm,target=/root/.npm \
    mkdir vendor && cp /tmp/drasi-react-0.1.0.tgz vendor/drasi-react.tgz && npm ci --no-audit --no-fund
COPY ${SOURCE_ROOT}/shared/ui/ ./
RUN npm run build

FROM rust:1.95-bookworm AS rust
ARG TARGETARCH
ARG SOURCE_ROOT=.
WORKDIR /app
COPY ${SOURCE_ROOT}/rust-toolchain.toml ./
COPY ${SOURCE_ROOT}/shared/Cargo.toml ${SOURCE_ROOT}/shared/Cargo.lock shared/
COPY ${SOURCE_ROOT}/shared/crates/ shared/crates/
COPY ${SOURCE_ROOT}/shared/policies/ shared/policies/
COPY ${SOURCE_ROOT}/shared/migrations/ shared/migrations/
COPY ${SOURCE_ROOT}/embedded/control/ embedded/control/
WORKDIR /app/shared
RUN --mount=type=cache,id=gpu-lab-registry,target=/usr/local/cargo/registry \
    --mount=type=cache,id=gpu-lab-control-linux-${TARGETARCH},target=/app/target \
    cargo build --locked --release --jobs 1 --target-dir /app/target -p gpu-control && cp /app/target/release/gpu-control /app/gpu-control

FROM debian:bookworm-slim
RUN apt-get update -qq && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=rust /app/gpu-control target/release/gpu-control
COPY --from=ui /ui/dist ui/dist
USER 65532:65532
CMD ["./target/release/gpu-control"]
