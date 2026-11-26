#!/usr/bin/env bash
# Sobe a infra compartilhada dos testes de integração.
#
# Rodado como setup script do nextest (`nextest.toml`): sobe Postgres/Redis uma
# vez por execução e publica as URLs em `$NEXTEST_ENV`. O harness cria um banco
# por teste dentro desse Postgres e usa um DB lógico por teste no Redis.
#
# Sem Docker (ou se o compose falhar), sai com sucesso e o harness cai no
# fallback de testcontainers, um par de containers por teste.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

if ! docker info >/dev/null 2>&1; then
    echo "testes: docker indisponível; usando testcontainers" >&2
    exit 0
fi

compose=(docker compose --project-directory . -f docker/compose.test.yml)

if ! "${compose[@]}" up -d --wait; then
    echo "testes: falha ao subir docker/compose.test.yml; usando testcontainers" >&2
    exit 0
fi

if [ -z "${NEXTEST_ENV:-}" ]; then
    echo "testes: NEXTEST_ENV ausente (rode via cargo nextest)" >&2
    exit 0
fi

postgres_port="${TEST_POSTGRES_PORT:-55433}"
redis_port="${TEST_REDIS_PORT:-56380}"

{
    echo "TEST_DATABASE_URL=postgres://app:app@127.0.0.1:${postgres_port}/app"
    echo "TEST_REDIS_URL=redis://127.0.0.1:${redis_port}"
} >> "$NEXTEST_ENV"
