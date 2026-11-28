# syntax=docker/dockerfile:1.7
# Imagem de produção: build multi-stage, binários estáticos no runtime mínimo,
# processo sem privilégios. `worker`, `migrator` e `admin` usam a mesma imagem.

FROM rust:1.99.1-slim-bookworm AS builder

WORKDIR /app

COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY src ./src

# Cache mounts mantêm registry e target entre builds; os binários são copiados
# para /out porque o conteúdo do cache mount não entra na camada.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo build --release --locked \
        --bin api --bin worker --bin migrator --bin admin \
    && mkdir -p /out \
    && cp target/release/api target/release/worker target/release/migrator \
          target/release/admin /out/

FROM debian:bookworm-slim AS runtime

# curl existe só para o HEALTHCHECK do compose (/health/ready no loopback).
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --system --gid 10001 app \
    && useradd --system --uid 10001 --gid app --home-dir /app --shell /usr/sbin/nologin app

WORKDIR /app

COPY --from=builder /out/api /usr/local/bin/api
COPY --from=builder /out/worker /usr/local/bin/worker
COPY --from=builder /out/migrator /usr/local/bin/migrator
COPY --from=builder /out/admin /usr/local/bin/admin
COPY default.toml ./default.toml
COPY config ./config

ENV APP_PROFILE=production
EXPOSE 8080

USER app

CMD ["api"]
