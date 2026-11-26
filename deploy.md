# Deploy

Guia de publicação do template. Para operação do dia a dia (backup, outbox,
incidentes e métricas), use [`docs/operations.md`](docs/operations.md).

## Imagem

O `docker/Dockerfile` é multi-stage:

- build em `rust:1.99.1-slim-bookworm`;
- runtime em `debian:bookworm-slim`;
- binários `api`, `worker` e `migrator`;
- processo roda como usuário `app` (UID 10001), sem privilégios.

`api` e `worker` usam a mesma imagem com comandos diferentes. `migrate` fica no
perfil `tools` do compose. Os arquivos ficam em [`docker/`](docker/):

| Arquivo | Uso |
|---|---|
| `docker/Dockerfile` | imagem de deploy |
| `docker/compose.local.yml` | Postgres/Redis/Mailpit no desenvolvimento |
| `docker/compose.deploy.yml` | stack de deploy |
| `docker/compose.test.yml` | Postgres/Redis efêmeros para debug manual |

```bash
just docker-build
# equivalente: docker build -f docker/Dockerfile -t template-backend-rust:local .
```

## Variáveis de produção

O `.env` **não é carregado em produção**: o `dotenvy` só roda no perfil
`development`. Injete os valores pelo ambiente, Docker secrets ou secret manager
do provedor.

| Variável | Obrigatória | Observação |
|---|---|---|
| `APP_PROFILE` | sim | use `production` |
| `APP__JWT__SECRET` | sim | mínimo 32 caracteres; trocar invalida todos os tokens |
| `APP__DATABASE__URL` | sim | URL do Postgres |
| `APP__REDIS__URL` | sim | URL do Redis |
| `APP__MAIL__PROVIDER` | sim | `smtp` em produção |
| `APP__MAIL__SMTP_URL` | com `smtp` | ex.: `smtps://user:senha@host:465` |
| `APP__MAIL__FROM` | sim | remetente válido |
| `APP__SERVER__TRUST_PROXY_HEADERS` | não | `true` só atrás de proxy que sobrescreve `X-Forwarded-For` |
| `APP__SECURITY__HSTS_MAX_AGE_SECS` | não | o perfil de produção usa `31536000` |
| `APP__DOCS__ENABLED` | não | produção desliga `/docs` e `/api-docs` |
| `APP__METRICS__ENABLED` | não | padrão `true` |
| `APP__METRICS__TOKEN` | recomendada | Bearer do `/metrics` (mín. 16); sem token, restrinja na rede |
| `APP__AUDIT__RETENTION_DAYS` | não | padrão 365; `0` mantém para sempre |

Também revise, em `config/production.toml` ou por ambiente:

- `cors.allowed_origins` — vazio por padrão; informe os domínios do front;
- `server.body_limit_mb` — 1 MB no perfil de produção;
- `server.worker_threads` — 2 no host de 2 vCPU;
- `jobs.timezone` — fuso dos jobs;
- `log.level` / `log.format` — JSON em produção.

Segredos nunca entram no repositório nem em TOML.

## Compose

O compose local sobe só a infraestrutura:

```bash
just up       # Postgres + Redis
just mail-up  # Mailpit
just down
```

O compose de deploy usa os perfis:

| Perfil | Serviços | Uso |
|---|---|---|
| (nenhum) | `postgres`, `redis` | dependências do deploy |
| `app` | `api`, `worker` | aplicação |
| `tools` | `migrate` | migrations explícitas |

O worker deve rodar em **uma instância só** para evitar disputa de lease do
outbox. A API pode ter mais de uma réplica atrás de um proxy.

Sempre use `--project-directory .` com o compose dentro de `docker/`, para
manter o `.env` da raiz e os caminhos relativos.

## Gate de produção

O workflow `.github/workflows/production.yml` concentra o build da imagem e o
smoke de compose. Ele **não** roda em PR nem em push de desenvolvimento.

Hoje o disparo é manual:

```bash
gh workflow run production.yml
```

Quando o projeto entrar em produção, descomente o gatilho de tag no workflow
para que cada `v*` passe pelo build e pelo compose antes do release.

## Passo a passo

```bash
# 1. schema (antes de subir a API nova)
docker compose --project-directory . -f docker/compose.deploy.yml \
  --profile tools run --rm --build migrate

# 2. aplicação
APP__JWT__SECRET="$(openssl rand -base64 48)" \
  docker compose --project-directory . -f docker/compose.deploy.yml \
  --profile app up -d --build api worker
```

Ou pelos atalhos:

```bash
just prod-migrate
APP__JWT__SECRET="$(openssl rand -base64 48)" just prod-up
```

Verificação:

```bash
curl -fsS http://127.0.0.1:8080/health
curl -fsS http://127.0.0.1:8080/health/ready
curl -fsS -H "authorization: Bearer $METRICS_TOKEN" \
  http://127.0.0.1:8080/metrics | head
```

Se `metrics.token` estiver vazio, o `/metrics` fica público; restrinja no proxy
ou na rede de monitoramento.

## Migrations

- São explícitas: nunca rodam no boot da API.
- Aplicar antes de subir a versão nova:
  `just prod-migrate` (ou `docker compose --project-directory . -f docker/compose.deploy.yml --profile tools run --rm --build migrate`).
- São **forward-only**. Rollback de imagem só funciona se a migration for
  compatível (coluna nullable/default ou expand/contract).
- Mudou o schema? Gere a entidade com `just entities` e teste em banco limpo.

## Smoke pós-deploy

O workflow `.github/workflows/smoke.yml` testa uma URL publicada, não containers
locais. Ele pode ser disparado manualmente ou chamado por `workflow_call` no
pipeline de deploy.

```bash
gh workflow run smoke.yml -f base_url=https://api.exemplo.com
```

Verificações:

- `/health` e `/health/ready`;
- cabeçalhos de segurança e `Cache-Control: no-store`;
- login inválido devolvendo `401`;
- `/metrics` quando o secret `METRICS_TOKEN` existe.

## Checklist de segurança

- [ ] `APP__JWT__SECRET` forte e fora do repositório.
- [ ] `APP__MAIL__PROVIDER=smtp` com `smtps://` real; Mailpit desligado.
- [ ] `docs.enabled = false` em produção pública.
- [ ] `/metrics` com token ou restrito à rede.
- [ ] CORS com os domínios reais, sem `*`.
- [ ] `trust_proxy_headers = true` só atrás de proxy confiável.
- [ ] HSTS ligado quando a API é servida por HTTPS.
- [ ] `audit.retention_days` compatível com a política de compliance.
- [ ] Backup e restore testados (`docs/operations.md`).
- [ ] Migration aplicada antes da API nova.

## Rollback

1. Volte a tag da imagem anterior.
2. Suba `api` e `worker` de novo.
3. Se a migration não for compatível, restaure o backup do banco.

Não rode `docker compose ... down -v` em produção: o `-v` apaga o volume
`pgdata`. Use `just prod-down`, que não usa `-v`.

## Runbook

Backup, restore, outbox, auditoria, métricas, rate limit e incidentes comuns
estão em [`docs/operations.md`](docs/operations.md).
