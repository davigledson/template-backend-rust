# Imagem de produção: build multi-stage, binários estáticos no runtime mínimo,
# processo sem privilégios. O serviço `worker` usa a mesma imagem com outro comando.

FROM rust:1.99.1-slim-bookworm AS builder

WORKDIR /app

COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY src ./src

RUN cargo build --release --locked --bin api --bin worker --bin migrator

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --system --gid 10001 app \
    && useradd --system --uid 10001 --gid app --home-dir /app --shell /usr/sbin/nologin app

WORKDIR /app

COPY --from=builder /app/target/release/api /usr/local/bin/api
COPY --from=builder /app/target/release/worker /usr/local/bin/worker
COPY --from=builder /app/target/release/migrator /usr/local/bin/migrator
COPY default.toml ./default.toml
COPY config ./config

ENV APP_PROFILE=production
EXPOSE 8080

USER app

CMD ["api"]
