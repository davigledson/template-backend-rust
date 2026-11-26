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

`cargo nextest run` **não** usa `compose.test.yml`: cada binário de teste sobe
seu próprio Postgres/Redis via testcontainers.

Use `compose.test.yml` apenas para inspeção manual:

```bash
docker compose --project-directory . -f docker/compose.test.yml up -d
docker compose --project-directory . -f docker/compose.test.yml down -v
```

Portas padrão: Postgres `55433`, Redis `56380`. Sem volume persistente.
