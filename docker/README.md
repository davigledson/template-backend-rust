# Docker

Arquivos de container do template. A API e o worker rodam **fora** do Docker no
desenvolvimento; o Compose local sobe só a infraestrutura.

## Arquivos

| Arquivo | Uso |
|---|---|
| `Dockerfile` | imagem de deploy: `api`, `worker`, `migrator` e `admin` |
| `compose.local.yml` | Postgres + Redis + Mailpit para desenvolvimento |
| `compose.deploy.yml` | Postgres + Redis + API + worker + migrate/admin para deploy |
| `compose.test.yml` | Postgres/Redis efêmeros para debug manual |

## Local

A partir da raiz do repositório:

```bash
just up          # Postgres + Redis
just mail-up     # adiciona Mailpit
just dev         # API fora do Docker
just worker      # worker fora do Docker
just down        # derruba a infra local
```

Sem `just`:

```bash
docker compose --project-directory . -f docker/compose.local.yml up -d
docker compose --project-directory . -f docker/compose.local.yml --profile mail up -d mailpit
docker compose --project-directory . -f docker/compose.local.yml down
```

O `--project-directory .` é importante: mantém o `.env` da raiz e os caminhos
relativos funcionando, mesmo com o compose dentro de `docker/`.

## Deploy

```bash
just docker-build
just prod-migrate
ADMIN_PASSWORD='uma-senha-bem-longa-123' \
  just prod-admin create --email admin@empresa.com --name "Admin"
just prod-up
```

Exporte `POSTGRES_PASSWORD`, `APP__DATABASE__URL` e `APP__JWT__SECRET` no shell.
As receitas `prod-*` usam `--env-file ${APP_ENV_FILE:-/dev/null}`, então o `.env`
de desenvolvimento não é herdado. Para um arquivo de produção:
`APP_ENV_FILE=.env.production just prod-up`.

Ou diretamente:

```bash
docker compose --project-directory . --env-file /dev/null \
  -f docker/compose.deploy.yml --profile tools run --rm --build migrate
docker compose --project-directory . --env-file /dev/null \
  -f docker/compose.deploy.yml --profile app up -d --build api worker
docker compose --project-directory . --env-file /dev/null \
  -f docker/compose.deploy.yml --profile app down
```

O build usa a raiz como contexto e `docker/Dockerfile`, com cache de dependências
via BuildKit:

```bash
docker build -f docker/Dockerfile -t template-backend-rust:local .
```

## Testes

Com `cargo nextest`, o setup script do `.config/nextest.toml` sobe este compose
**uma vez** por execução e publica as URLs em `TEST_DATABASE_URL` e
`TEST_REDIS_URL`. Cada teste ganha um banco próprio no Postgres e um DB lógico
próprio no Redis (`--databases 64`), então roda em paralelo sem estado
compartilhado. A infra fica de pé entre execuções; `just test-down` derruba.

Sem nextest (ex.: `cargo test`), o harness ignora o compose e usa testcontainers.
Para inspecionar o banco à mão:

```bash
docker compose --project-directory . -f docker/compose.test.yml up -d
docker compose --project-directory . -f docker/compose.test.yml down -v
```

Portas padrão: Postgres `55433`, Redis `56380`. Sem volume persistente.
