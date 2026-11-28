#!/usr/bin/env bash
# Benchmark reproduzível dos endpoints do template.
#
# Uso:
#   BENCH_EMAIL=admin@example.com BENCH_PASSWORD='...' just bench [duração] [conexões]
#   BENCH_EMAIL=... BENCH_PASSWORD=... ./scripts/bench.sh 10s 20
#
# Requer a API de pé e o Postgres/Redis do compose local. Prefere `oha`
# (p50/p95/p99 no JSON) e cai para `ab` quando ele não existe.
set -euo pipefail

usage() {
    cat <<'EOF'
Uso: scripts/bench.sh [duração] [conexões]

Variáveis:
  BENCH_BASE_URL       URL da API (padrão: http://127.0.0.1:8080)
  BENCH_EMAIL          e-mail de um admin (obrigatório)
  BENCH_PASSWORD       senha do admin (obrigatório)
  BENCH_TOOL           oha ou ab (padrão: oha, se instalado; senão ab)
  BENCH_DURATION       duração por endpoint (padrão: 10s)
  BENCH_CONCURRENCY    conexões simultâneas (padrão: 20)

Exemplo:
  just admin-create bench@example.com "Bench" true
  BENCH_EMAIL=bench@example.com BENCH_PASSWORD='...' just bench 10s 20
EOF
}

if [ "${1:-}" = "-h" ] || [ "${1:-}" = "--help" ]; then
    usage
    exit 0
fi

base_url="${BENCH_BASE_URL:-http://127.0.0.1:8080}"
base_url="${base_url%/}"
email="${BENCH_EMAIL:-}"
password="${BENCH_PASSWORD:-}"
duration="${1:-${BENCH_DURATION:-10s}}"
concurrency="${2:-${BENCH_CONCURRENCY:-20}}"

if [ -z "$email" ] || [ -z "$password" ]; then
    usage >&2
    echo "erro: defina BENCH_EMAIL e BENCH_PASSWORD" >&2
    exit 2
fi

if ! [[ "$duration" =~ ^[0-9]+s?$ ]]; then
    echo "erro: duração deve ser segundos (10) ou segundos com sufixo (10s)" >&2
    exit 2
fi

if ! [[ "$concurrency" =~ ^[0-9]+$ ]] || [ "$concurrency" -lt 1 ]; then
    echo "erro: conexões deve ser um inteiro positivo" >&2
    exit 2
fi

duration_secs="${duration%s}"
duration="${duration_secs}s"

tool="${BENCH_TOOL:-}"
if [ -z "$tool" ]; then
    if command -v oha >/dev/null 2>&1; then
        tool="oha"
    elif command -v ab >/dev/null 2>&1; then
        tool="ab"
    else
        echo "erro: instale oha ou ab para rodar o benchmark" >&2
        echo "  oha: https://github.com/hatoo/oha (ou cargo install oha)" >&2
        exit 2
    fi
fi

case "$tool" in
    oha|ab) ;;
    *)
        echo "erro: BENCH_TOOL deve ser oha ou ab" >&2
        exit 2
        ;;
esac

if [ "$tool" = "oha" ] && ! command -v oha >/dev/null 2>&1; then
    echo "erro: oha não encontrado no PATH" >&2
    exit 2
fi

if [ "$tool" = "ab" ] && ! command -v ab >/dev/null 2>&1; then
    echo "erro: ab não encontrado no PATH" >&2
    exit 2
fi

for dependency in curl python3; do
    command -v "$dependency" >/dev/null 2>&1 \
        || {
            echo "erro: $dependency é necessário" >&2
            exit 2
        }
done

json_field() {
    # Lê o JSON do stdin e imprime o campo pedido (string vazia se ausente).
    python3 -c '
import json
import sys

field = sys.argv[1]
try:
    data = json.load(sys.stdin)
except json.JSONDecodeError:
    sys.exit(0)

value = data
for part in field.split("."):
    if not isinstance(value, dict):
        sys.exit(0)
    value = value.get(part)

if value is not None:
    print(value)
' "$1"
}

if ! curl -fsS --max-time 5 "$base_url/health/ready" >/dev/null; then
    echo "erro: $base_url/health/ready não respondeu; suba a API e a infra" >&2
    exit 1
fi

login_response="$(curl -sS --max-time 10 -X POST "$base_url/api/v1/auth/login" \
    -H 'content-type: application/json' \
    -d "{\"email\":\"$email\",\"password\":\"$password\"}")" || {
    echo "erro: login falhou (a API está de pé?)" >&2
    exit 1
}
access_token="$(printf '%s' "$login_response" | json_field accessToken)"
if [ -z "$access_token" ]; then
    echo "erro: login não devolveu accessToken; confira BENCH_EMAIL/BENCH_PASSWORD" >&2
    exit 1
fi

me_response="$(curl -fsS --max-time 10 "$base_url/api/v1/auth/me" \
    -H "authorization: Bearer $access_token")"
user_id="$(printf '%s' "$me_response" | json_field id)"

parse_oha() {
    python3 -c '
import json
import sys

try:
    data = json.load(sys.stdin)
except json.JSONDecodeError:
    print("- - - - -")
    raise SystemExit

summary = data.get("summary", {})
percentiles = data.get("latencyPercentiles") or data.get("latency_percentiles") or {}

def millis(key):
    value = percentiles.get(key)
    return "-" if value is None else f"{float(value) * 1000:.1f}"

codes = summary.get("httpCodes") or summary.get("http_codes") or {}
total = float(summary.get("total") or 0)
ok = float(codes.get("200") or 0)
rps = float(summary.get("requestsPerSecond") or 0)
p50 = millis("p50")
p95 = millis("p95")
p99 = millis("p99")
print(f"{rps:.1f} {p50} {p95} {p99} {int(max(total - ok, 0))}")
'
}

parse_ab() {
    awk '
        /^Requests per second:/ { rps = $4 }
        /^Failed requests:/ { failed = $3 }
        /^[[:space:]]*50%/ { p50 = $2 }
        /^[[:space:]]*95%/ { p95 = $2 }
        /^[[:space:]]*99%/ { p99 = $2 }
        END {
            printf "%s %s %s %s %s\n",
                (rps == "" ? "-" : rps),
                (p50 == "" ? "-" : p50),
                (p95 == "" ? "-" : p95),
                (p99 == "" ? "-" : p99),
                (failed == "" ? 0 : failed)
        }
    '
}

run_benchmark() {
    local name="$1" url="$2" auth="$3"
    local -a auth_header=()
    if [ -n "$auth" ]; then
        auth_header=(-H "authorization: Bearer $auth")
    fi

    local status
    status="$(curl -sS -o /dev/null -w '%{http_code}' --max-time 10 \
        "${auth_header[@]}" "$url")"
    if [ "$status" != "200" ]; then
        echo "erro: $name devolveu $status no aquecimento (esperado 200)" >&2
        exit 1
    fi

    local rps p50 p95 p99 errors
    if [ "$tool" = "oha" ]; then
        local output
        output="$(oha -z "$duration" -c "$concurrency" -j --no-tui \
            "${auth_header[@]}" "$url")"
        read -r rps p50 p95 p99 errors <<<"$(printf '%s' "$output" | parse_oha)"
    else
        local output
        output="$(ab -q -t "$duration_secs" -c "$concurrency" \
            "${auth_header[@]}" "$url" 2>&1)"
        read -r rps p50 p95 p99 errors <<<"$(printf '%s\n' "$output" | parse_ab)"
    fi

    printf '%-28s %8s %8s %8s %8s %7s\n' \
        "$name" "$rps" "$p50" "$p95" "$p99" "$errors"
}

printf 'bench: %s (ferramenta=%s, duração=%s, conexões=%s)\n' \
    "$base_url" "$tool" "$duration" "$concurrency"
printf '%-28s %8s %8s %8s %8s %7s\n' \
    "endpoint" "rps" "p50ms" "p95ms" "p99ms" "erros"

run_benchmark "GET /health" "$base_url/health" ""
run_benchmark "GET /auth/me" "$base_url/api/v1/auth/me" "$access_token"
run_benchmark "GET /users?page=1" "$base_url/api/v1/users?page=1&perPage=20" "$access_token"
if [ -n "$user_id" ]; then
    run_benchmark "GET /users/{id}" "$base_url/api/v1/users/$user_id" "$access_token"
fi
run_benchmark "GET /audit?page=1" "$base_url/api/v1/audit?page=1&perPage=20" "$access_token"

cat <<'EOF'

Notas:
- O login não entra no benchmark: um POST por iteração cria sessões e esbarra
  no rate limit. Meça /auth/login à parte, com o rate limit desligado.
- O número vale para este host e para a massa de dados atual. Registre data,
  CPU, memória e quantidade de linhas junto do resultado.
- Compare sempre build release (`cargo run --release --bin api`) com o mesmo
  Postgres. Debug não serve para decisão de capacidade.
EOF
