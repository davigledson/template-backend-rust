#!/usr/bin/env bash
# Smoke pós-deploy: health, cabeçalhos, login inválido e /metrics.
#
# Usado pelo workflow `smoke.yml` e por `just smoke <url> [token]`. Roda contra
# uma URL publicada, não contra containers locais.
set -euo pipefail

usage() {
    echo "Uso: just smoke <base_url> [metrics_token]" >&2
    echo "  <base_url>       ex.: https://api.exemplo.com" >&2
    echo "  [metrics_token]  Bearer do /metrics (obrigatório em produção)" >&2
}

if [ "$#" -lt 1 ] || [ -z "$1" ]; then
    usage
    exit 2
fi

base_url="${1%/}"
metrics_token="${2:-}"

echo "smoke: $base_url"

curl -fsS --retry 3 --retry-delay 2 "$base_url/health" >/dev/null
curl -fsS --retry 5 --retry-delay 2 "$base_url/health/ready" >/dev/null

headers="$(curl -sS -D - -o /dev/null "$base_url/api/v1/users")"
printf '%s' "$headers" | grep -qi '^x-content-type-options: nosniff' \
    || {
        echo "smoke: x-content-type-options: nosniff ausente" >&2
        exit 1
    }
printf '%s' "$headers" | grep -qi '^cache-control: no-store' \
    || {
        echo "smoke: cache-control: no-store ausente" >&2
        exit 1
    }

status="$(curl -sS -o /dev/null -w '%{http_code}' \
    -X POST "$base_url/api/v1/auth/login" \
    -H 'content-type: application/json' \
    -d '{"email":"smoke-nao-existe@example.invalid","password":"senha-errada-123"}')"
if [ "$status" != "401" ]; then
    echo "smoke: login inválido devolveu $status (esperado 401)" >&2
    exit 1
fi

if [ -z "$metrics_token" ]; then
    echo "smoke: sem token de métricas; /metrics não foi checado"
else
    curl -fsS -H "authorization: Bearer $metrics_token" "$base_url/metrics" \
        | grep -q 'app_email_outbox_pending' \
        || {
            echo "smoke: /metrics sem app_email_outbox_pending" >&2
            exit 1
        }
fi

echo "smoke ok"
