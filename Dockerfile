# syntax=docker/dockerfile:1
#
# 聚合台 single image: Rust backend + built frontend + pinned catbus runtime.
#
#   docker compose up -d --build
#
# Behind a slow/blocked network, pass mirrors as build args, e.g.
#   docker compose build \
#     --build-arg NPM_REGISTRY=https://registry.npmmirror.com \
#     --build-arg CARGO_MIRROR=sparse+https://rsproxy.cn/index/

ARG NODE_IMAGE=node:22-bookworm-slim
ARG RUST_IMAGE=rust:1-bookworm

# ---- frontend: static bundle (relative asset URLs, works under any prefix) ----
FROM ${NODE_IMAGE} AS frontend
ARG NPM_REGISTRY=https://registry.npmjs.org
WORKDIR /src/frontend
COPY frontend/package.json frontend/package-lock.json ./
RUN npm ci --registry="$NPM_REGISTRY" --no-audit --no-fund
COPY frontend/ ./
RUN npm run build

# ---- backend: release binary ----
FROM ${RUST_IMAGE} AS backend
ARG CARGO_MIRROR=
# aws-lc-sys (rustls) may fall back to a CMake build.
RUN apt-get update && apt-get install -y --no-install-recommends cmake && rm -rf /var/lib/apt/lists/*
RUN if [ -n "$CARGO_MIRROR" ]; then \
      mkdir -p "$CARGO_HOME" && printf '[source.crates-io]\nreplace-with = "mirror"\n[source.mirror]\nregistry = "%s"\n' "$CARGO_MIRROR" > "$CARGO_HOME/config.toml"; \
    fi
WORKDIR /src/backend
COPY backend/Cargo.toml backend/Cargo.lock ./
# Build dependencies first so source edits don't recompile the world.
RUN mkdir src && echo 'fn main() {}' > src/main.rs && cargo build --release --locked && rm -rf src
COPY backend/src ./src
RUN touch src/main.rs && cargo build --release --locked

# ---- catbus runtime: production node_modules only ----
FROM ${NODE_IMAGE} AS runtime
ARG NPM_REGISTRY=https://registry.npmjs.org
WORKDIR /app/runtime
COPY runtime/package.json runtime/package-lock.json ./
RUN npm ci --omit=dev --registry="$NPM_REGISTRY" --no-audit --no-fund

# ---- final image ----
FROM ${NODE_IMAGE}
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates tini tzdata \
 && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=runtime /app/runtime/node_modules runtime/node_modules
COPY --from=frontend /src/frontend/dist frontend/dist
COPY --from=backend /src/backend/target/release/mpf-server /usr/local/bin/mpf-server
RUN mkdir -p /app/data && chown node:node /app/data

ENV MPF_ROOT=/app \
    MPF_BIND=0.0.0.0:8787 \
    MPF_DATA_DIR=/app/data \
    MPF_STATIC_DIR=/app/frontend/dist \
    MPF_CATBUS_BIN=/app/runtime/node_modules/.bin/catbus \
    RUST_LOG=info \
    TZ=Asia/Shanghai

# uid/gid 1000: the mounted data directory must be writable by it.
USER node
VOLUME /app/data
EXPOSE 8787
HEALTHCHECK --interval=30s --timeout=5s --start-period=20s --retries=3 \
  CMD node -e "fetch('http://127.0.0.1:8787/healthz').then(r=>process.exit(r.ok?0:1),()=>process.exit(1))"
# tini reaps the catbus child processes and forwards signals.
ENTRYPOINT ["/usr/bin/tini", "--"]
CMD ["mpf-server"]
