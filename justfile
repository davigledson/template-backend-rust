set shell := ["bash", "-uc"]
set dotenv-load := true

profile := env("APP_PROFILE", "development")
db_url := env("APP__DATABASE__URL", "postgres://app:app@localhost:55432/app")

# Lista os comandos disponíveis.
default:
    @just --list

# Prepara o ambiente local: .env, infra e dependências.
setup:
    @test -f .env || cp .env.example .env
    @grep -q '^APP__JWT__SECRET=.\+' .env || echo "AVISO: preencha APP__JWT__SECRET no .env (openssl rand -base64 48)"
    cargo fetch
    docker compose up -d
    @just doctor

# Mostra versões e o estado da infraestrutura.
doctor:
    @echo "profile   {{profile}}"
    @echo "rustc     $(rustc --version)"
    @echo "cargo     $(cargo --version)"
    @echo "just      $(just --version | cut -d' ' -f2)"
    @echo "compose   $(docker compose version --short)"
    @docker compose ps
    @test -f .env && echo ".env      ok" || echo ".env      ausente (rode: just setup)"

# Sobe Postgres e Redis em background.
up:
    docker compose up -d

# Derruba a infraestrutura (mantém os volumes).
down:
    docker compose down

# Derruba a infraestrutura e apaga os volumes.
reset:
    docker compose down -v

# Acompanha os logs da infraestrutura.
logs:
    docker compose logs -f --tail=100

# Sobe a API com logs legíveis.
dev:
    cargo run --bin api

# Sobe o worker (cron).
worker:
    cargo run --bin worker

# Aplica as migrations pendentes.
migrate:
    cargo run --bin migrator

# Cria um administrador (o primeiro do banco, ou outro com --force).
admin-create email name force="false":
    cargo run --quiet --bin admin -- create --email {{email}} --name "{{name}}" {{ if force == "true" { "--force" } else { "" } }}

# Promove um usuário existente a administrador.
admin-promote email:
    cargo run --quiet --bin admin -- promote --email {{email}}

# Redefine a senha de um usuário (invalida as sessões).
admin-reset-password email:
    cargo run --quiet --bin admin -- reset-password --email {{email}}

# Verifica o build de todos os alvos.
check:
    cargo check --all-targets --all-features

# Formata o código.
fmt:
    cargo fmt --all

# Roda o clippy tratando warnings como erro.
lint:
    cargo clippy --all-targets --all-features -- -D warnings

# Roda todos os testes (unit + integração).
test:
    cargo nextest run

# Roda apenas os testes unitários da lib.
test-unit:
    cargo nextest run --lib

# Roda os testes com saída detalhada.
test-verbose:
    cargo nextest run --no-capture

# Gera as entidades SeaORM a partir do banco atual.
entities:
    PATH="$HOME/.cargo/bin:$PATH" sea-orm-cli generate entity \
        --database-url "{{db_url}}" \
        --output-dir src/database/entity \
        --with-serde both \
        --with-prelude none \
        --date-time-crate chrono

# Verifica o snapshot do OpenAPI (Fase 4).
openapi-check:
    cargo insta test --check

# Pipeline local equivalente ao CI.
ci: fmt lint check test
