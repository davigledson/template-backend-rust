set shell := ["bash", "-uc"]

profile := env("APP_PROFILE", "development")
compose_local := "docker compose --project-directory . -f docker/compose.local.yml"
compose_test := "docker compose --project-directory . -f docker/compose.test.yml"
compose_deploy := "docker compose --project-directory . --env-file ${APP_ENV_FILE:-/dev/null} -f docker/compose.deploy.yml"

# Lista os comandos disponíveis.
default:
    @just --list

# Prepara o ambiente local: .env, infra e dependências.
setup:
    @test -f .env || cp .env.example .env
    @grep -q '^APP__JWT__SECRET=.\+' .env || echo "AVISO: preencha APP__JWT__SECRET no .env (openssl rand -base64 48)"
    cargo fetch
    {{compose_local}} up -d
    @just doctor

# Mostra versões e o estado da infraestrutura.
doctor:
    @echo "profile   {{profile}}"
    @echo "rustc     $(rustc --version)"
    @echo "cargo     $(cargo --version)"
    @echo "just      $(just --version | cut -d' ' -f2)"
    @echo "compose   $(docker compose version --short)"
    @command -v lld >/dev/null 2>&1 && echo "lld       ok" || echo "lld       ausente (instale para o linker do .cargo/config.toml)"
    @{{compose_local}} ps
    @test -f .env && echo ".env      ok" || echo ".env      ausente (rode: just setup)"

# Renomeia o projeto do template (crate, imports, compose, tags, OpenAPI e JWT).
rename *args:
    ./scripts/rename-project.sh {{args}}

# Sobe Postgres e Redis em background.
up:
    {{compose_local}} up -d

# Derruba a infraestrutura (mantém os volumes).
down:
    {{compose_local}} down

# Derruba a infraestrutura e apaga os volumes.
reset:
    {{compose_local}} down -v

# Acompanha os logs da infraestrutura.
logs:
    {{compose_local}} logs -f --tail=100

# Cria pg_stat_statements e mostra as consultas mais caras e as em execução.
db-stats:
    @set -a; [ -f .env ] && . ./.env; set +a; \
      psql "${APP__DATABASE__URL:-postgres://app:app@localhost:55432/app}" -v ON_ERROR_STOP=1 \
        -c "create extension if not exists pg_stat_statements" \
        -c "select round(total_exec_time::numeric, 1) as total_ms, calls, round(mean_exec_time::numeric, 2) as mean_ms, rows, left(query, 100) as query from pg_stat_statements order by total_exec_time desc limit 15" \
        -c "select pid, now() - query_start as duration, state, left(query, 100) as query from pg_stat_activity where state <> 'idle' and query_start is not null order by duration desc limit 10"

# Sobe a API com logs legíveis.
dev:
    cargo run --bin api

# Sobe o worker (cron).
worker:
    cargo run --bin worker

# Sobe o Mailpit para testar e-mail local (SMTP 1025, UI 8025).
mail-up:
    {{compose_local}} --profile mail up -d mailpit
    @echo "Mailpit: http://localhost:${MAILPIT_UI_PORT:-8025} (SMTP em localhost:${MAILPIT_SMTP_PORT:-1025})"

# Para o Mailpit (mantém o container para reiniciar depois).
mail-down:
    {{compose_local}} --profile mail stop mailpit

# Aplica as migrations pendentes.
migrate:
    cargo run --bin migrator

# Gera uma migration vazia e registra no mod.rs.
migration name:
    PATH="$HOME/.cargo/bin:$PATH" sea-orm-cli migrate generate {{name}} -d src/migrations

# Gera o esqueleto de um módulo (rota 501, router e OpenAPI).
module *args:
    ./scripts/new-module.sh {{args}}

# Cria um administrador (o primeiro do banco, ou outro com --force).
admin-create email name force="false":
    cargo run --quiet --bin admin -- create --email {{email}} --name "{{name}}" {{ if force == "true" { "--force" } else { "" } }}

# Promove um usuário existente a administrador.
admin-promote email:
    cargo run --quiet --bin admin -- promote --email {{email}}

# Redefine a senha de um usuário (invalida as sessões).
admin-reset-password email:
    cargo run --quiet --bin admin -- reset-password --email {{email}}

# Confirma o e-mail de um usuário (rollout de require_email_verified).
admin-verify-email email:
    cargo run --quiet --bin admin -- verify-email --email {{email}}

# Confirma o e-mail de todos os usuários pendentes (backfill).
admin-verify-email-all:
    cargo run --quiet --bin admin -- verify-email --all

# Lista e-mails do outbox por status (pending, failed ou sent).
outbox-list status="failed" limit="20":
    cargo run --quiet --bin admin -- outbox list --status {{status}} --limit {{limit}}

# Devolve um e-mail do outbox ao processamento imediato.
outbox-retry id:
    cargo run --quiet --bin admin -- outbox retry {{id}}

# Reprocessa todos os e-mails de um status; force="true" zera as tentativas.
outbox-retry-all status="failed" force="false":
    cargo run --quiet --bin admin -- outbox retry-all --status {{status}} {{ if force == "true" { "--force" } else { "" } }}

# Verifica o build de todos os alvos.
check:
    cargo check --all-targets --all-features

# Formata o código.
fmt:
    cargo fmt --all

# Roda o clippy tratando warnings como erro.
lint:
    cargo clippy --all-targets --all-features -- -D warnings

# Roda todos os testes (unit + integração). O nextest sobe a infra de teste uma
# vez e a mantém de pé; use `just test-down` para derrubar.
test:
    cargo nextest run

# Derruba a infra compartilhada dos testes de integração.
test-down:
    {{compose_test}} down -v --remove-orphans

# Roda apenas os testes unitários da lib.
test-unit:
    cargo nextest run --lib

# Roda os testes com saída detalhada.
test-verbose:
    cargo nextest run --no-capture

# Gera as entidades SeaORM a partir do banco atual.
entities:
    @set -a; [ -f .env ] && . ./.env; set +a; \
      PATH="$HOME/.cargo/bin:$PATH" sea-orm-cli generate entity \
        --database-url "${APP__DATABASE__URL:-postgres://app:app@localhost:55432/app}" \
        --output-dir src/database/entity \
        --with-serde both \
        --with-prelude none \
        --date-time-crate chrono

# Verifica o snapshot do OpenAPI (falha se o contrato mudar sem aceite).
openapi-check:
    cargo test --lib routes::openapi

# Verifica licenças, advisories e fontes das dependências.
deny:
    cargo deny check

# Sobe a stack de deploy (infra + api + worker).
prod-up:
    {{compose_deploy}} --profile app up -d --build api worker

# Sobe a SPA do front no mesmo servidor (imagem em FRONT_IMAGE).
prod-front:
    {{compose_deploy}} --profile front up -d front

# Derruba a stack de deploy.
prod-down:
    {{compose_deploy}} --profile app --profile front down

# Aplica as migrations na stack de deploy.
prod-migrate:
    {{compose_deploy}} --profile tools run --rm --build migrate

# Roda o CLI admin na stack de deploy (create, promote, reset-password...).
prod-admin +args:
    {{compose_deploy}} --profile tools run --rm admin {{args}}

# Acompanha os logs da stack de deploy (API, worker e front).
prod-logs:
    {{compose_deploy}} --profile app --profile front logs -f --tail=100

# Smoke pós-deploy (health, cabeçalhos, login inválido e /metrics).
smoke base_url metrics_token="":
    ./scripts/smoke.sh "{{base_url}}" "{{metrics_token}}"

# Mede throughput e latência dos endpoints principais (precisa de oha ou ab).
bench duration="10s" concurrency="20":
    ./scripts/bench.sh "{{duration}}" "{{concurrency}}"

# Constrói a imagem de produção.
docker-build:
    docker build -f docker/Dockerfile -t template-backend-rust:local .

# Pipeline local completo (mesmo conjunto do job quality).
ci: fmt lint check test deny

# Feedback rápido do loop local: sem containers.
ci-fast:
    cargo fmt --all -- --check
    cargo clippy --all-targets --all-features -- -D warnings
    cargo check --all-targets --all-features
    cargo nextest run --lib

# Verificação completa antes de push: integração + deny.
ci-full:
    cargo fmt --all -- --check
    cargo clippy --all-targets --all-features -- -D warnings
    cargo nextest run
    cargo deny check

# Instala hooks de pre-commit/pre-push neste clone.
hooks:
    ./scripts/install-git-hooks.sh
