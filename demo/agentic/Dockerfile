# Agentic demo image: real services (PostgreSQL, Redis, nginx, a Spring-style
# "order-api") running in one container, discovered by the AppControl agent and
# rendered as an architecture map by the AI layer.
#
# Build context is the repository root:
#   docker build -f demo/agentic/Dockerfile -t appcontrol-agentic-demo .

# ---- Stage 1: build the two binaries ----
FROM rust:1-bookworm AS builder
WORKDIR /src
COPY . .
# GIT_HASH / BUILD_TIME are injected by build.rs in some crates; provide safe
# fallbacks so the release build doesn't depend on a .git directory.
ENV GIT_HASH=demo BUILD_TIME=demo
RUN cargo build --release -p appcontrol-agent -p appcontrol-ai

# ---- Stage 2: runtime with real services ----
FROM debian:bookworm-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      postgresql redis-server nginx python3 procps iproute2 ca-certificates \
 && rm -rf /var/lib/apt/lists/*

COPY --from=builder /src/target/release/appcontrol-agent /usr/local/bin/appcontrol-agent
COPY --from=builder /src/target/release/appcontrol-ai    /usr/local/bin/appcontrol-ai

RUN mkdir -p /opt/order-api/config
COPY demo/agentic/order-api.py /opt/order-api/order-api.py
COPY demo/agentic/application.yml /opt/order-api/config/application.yml
COPY demo/agentic/entrypoint.sh /usr/local/bin/demo-entrypoint.sh
RUN chmod +x /usr/local/bin/demo-entrypoint.sh

ENTRYPOINT ["/usr/local/bin/demo-entrypoint.sh"]
