# Docker

Arquivos de container do template. A API e o worker rodam **fora** do Docker no
desenvolvimento; o Compose local sobe só a infraestrutura.

## Arquivos

| Arquivo | Uso |
|---|---|
| `Dockerfile` | imagem de deploy: `api`, `worker`, `migrator` e `admin` |
| `compose.local.yml` | Postgres + Redis + Mailpit para desenvolvimento |
| `compose.deploy.yml` | Postgres + Redis + API + worker + front + migrate/admin para deploy |
| `compose.front-build.yml` | override opcional para buildar o front localmente |
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

### Front no mesmo servidor

O perfil `front` sobe a SPA estática no mesmo domínio da API. Em produção a
imagem vem do CI do repositório do front:

```bash
export FRONT_IMAGE=ghcr.io/org/front:2026-11-28
docker compose --project-directory . --env-file /dev/null \
  -f docker/compose.deploy.yml --profile app --profile front \
  up -d --remove-orphans
```

Para buildar localmente, use o override (o repositório do front precisa estar em
`FRONT_CONTEXT`, padrão `../react-vite-template`):

```bash
FRONT_IMAGE=app-front:local \
VITE_API_URL=https://app.exemplo.com/api/v1 \
docker compose --project-directory . --env-file /dev/null \
  -f docker/compose.deploy.yml -f docker/compose.front-build.yml \
  --profile front up -d --build front
```

`api` e `front` publicam só em `127.0.0.1`. Quem roteia `/` e `/api/` é o nginx
do host; o server block e os detalhes de cookie estão em `deploy.md`.

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
