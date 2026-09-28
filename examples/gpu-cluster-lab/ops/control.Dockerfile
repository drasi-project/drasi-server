FROM node:24-bookworm-slim AS ui
ARG NPM_REGISTRY=https://registry.npmjs.org/
ENV npm_config_registry=$NPM_REGISTRY
WORKDIR /react-package
COPY .build/npm-cache/ /npm-cache/
COPY .build/react-source/dev-tools/react/ ./
RUN --mount=type=cache,id=gpu-lab-npm,target=/root/.npm \
    set -- /npm-cache/*.tgz; \
    if [ -f "$1" ]; then npm cache add --offline --ignore-scripts "$@"; fi; \
    npm ci --ignore-scripts --no-audit --no-fund && npm run build && npm pack --ignore-scripts --pack-destination /tmp
WORKDIR /ui
COPY ui/package.json ui/package-lock.json ./
RUN --mount=type=cache,id=gpu-lab-npm,target=/root/.npm \
    mkdir vendor && cp /tmp/drasi-react-0.1.0.tgz vendor/drasi-react.tgz && npm ci --no-audit --no-fund
COPY ui/ ./
RUN npm run build

FROM rust:1.95-bookworm AS rust
ARG TARGETARCH
WORKDIR /app
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates/ crates/
COPY policies/ policies/
COPY migrations/ migrations/
RUN --mount=type=cache,id=gpu-lab-registry,target=/usr/local/cargo/registry \
    --mount=type=cache,id=gpu-lab-control-linux-${TARGETARCH},target=/app/target \
    cargo build --locked --release --jobs 1 -p gpu-control && cp target/release/gpu-control /app/gpu-control

FROM debian:bookworm-slim
RUN apt-get update -qq && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=rust /app/gpu-control target/release/gpu-control
COPY --from=ui /ui/dist ui/dist
USER 65532:65532
CMD ["./target/release/gpu-control"]
