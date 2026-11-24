# template-backend-rust

Template de backend em **Rust** para projetos novos: Axum + SeaORM + PostgreSQL + Redis,
com JWT próprio, OpenAPI, e-mail transacional por *outbox*, cron em worker separado
e configuração em camadas.

Arquitetura: **monólito modular por feature** — uma única API (`/api/v1`) + um worker,
módulos donos das suas rotas/regras/tabelas e um `core` fino de infraestrutura.

> **Trabalhando no projeto (pessoa ou agente)?** Leia o
> [`AGENTS.md`](AGENTS.md): regras duras, comandos e checklists. O "porquê" está
> em [`docs/`](docs/):
> [arquitetura](docs/architecture.md) ·
> [adicionar módulo](docs/adding-a-module.md) ·
> [segurança](docs/security.md) ·
> [testes](docs/testing.md) ·
> [decisões](docs/decisions.md).

## Pré-requisitos (máquina do dev)

| Ferramenta | Obrigatória | Observação |
|---|---|---|
| `rustup` | sim | a versão do projeto está em `rust-toolchain.toml` |
| Docker + Compose | sim | sobe Postgres e Redis; não é preciso instalar banco local |
| `just` | sim | todos os comandos do projeto |
| `cargo-nextest` | recomendada | `just test` |
| `sea-orm-cli` 2.0.4 | só ao mexer no schema | `just entities` |
| `cargo-insta`, `cargo-deny` | CI/snapshots | OpenAPI e licenças |

```bash
# Arch/CachyOS
sudo pacman -S --needed base-devel pkg-config just cargo-nextest cargo-deny
cargo install sea-orm-cli --version 2.0.4 --locked
```

## Começando

```bash
cp .env.example .env          # preencha APP__JWT__SECRET
just setup                    # .env + infra + dependências
just migrate                  # aplica as migrations
just admin-create admin@empresa.com "Admin"   # bootstrap do primeiro admin
just dev                      # API em http://localhost:8080
```

> O primeiro admin é criado por CLI, nunca por rota pública nem por seed com
> senha no repositório. Em automação, use `ADMIN_PASSWORD=... just admin-create ...`.

> As portas do host são configuráveis no `.env` (`POSTGRES_PORT`, `REDIS_PORT`)
> e por padrão usam **55432** e **56379**, para não colidir com outros projetos
> que já ocupam 5432/6379.

## Comandos

| Comando | Faz |
|---|---|
| `just setup` | prepara `.env`, sobe a infra e baixa dependências |
| `just doctor` | versões, estado dos containers e do `.env` |
| `just up` / `just down` / `just reset` | infra (reset apaga volumes) |
| `just dev` | API com logs legíveis |
| `just worker` | entrega do outbox de e-mail e limpeza (cron) |
| `just migrate` | migrations pendentes |
| `just admin-create <email> "<nome>" [true]` | cria o primeiro admin (ou outro com `true`) |
| `just admin-promote <email>` | promove usuário existente a admin |
| `just admin-reset-password <email>` | redefine a senha e invalida as sessões |
| `just test` | testes unitários + integração |
| `just fmt` / `just lint` / `just check` | qualidade |
| `just openapi-check` | snapshot do contrato OpenAPI |
| `just deny` | licenças, advisories e fontes das dependências |
| `just entities` | gera entidades a partir do banco |
| `just docker-build` | imagem de produção multi-stage |
| `just ci` | fmt + lint + check + test + deny |

## Configuração

Precedência (do menor para o maior):

1. `default.toml` — padrões versionados;
2. `config/<perfil>.toml` — overrides por ambiente;
3. variáveis `APP__*` — `APP__DATABASE__URL` vira `database.url`;
4. `.env` — **apenas em desenvolvimento**, nunca versionado.

Segredos nunca ficam em TOML. `APP__JWT__SECRET` é obrigatório e precisa ter
no mínimo 32 caracteres (validado no boot).

E-mail:

| Chave | Padrão | O que faz |
|---|---|---|
| `mail.provider` | `log` | `log` registra o envio sem enviar; `smtp` envia de verdade |
| `mail.from` | `nao-responda@example.com` | remetente |
| `mail.smtp_url` | vazio | obrigatório com `provider = smtp` (ex.: `smtps://user:senha@host:465`) |

Em desenvolvimento use `log`; a senha do SMTP fica só no ambiente
(`APP__MAIL__SMTP_URL`) e é redigida no `Debug` da configuração.

Rate limit e proxy:

| Chave | Padrão | O que faz |
|---|---|---|
| `rate_limit.login_account_attempts` | 5 | falhas por conta na janela |
| `rate_limit.login_ip_attempts` | 20 | falhas por IP na janela |
| `rate_limit.login_window_secs` | 900 | janela fixa de 15 min |
| `server.trust_proxy_headers` | `false` | lê `X-Forwarded-For`/`X-Real-IP` |

O rate limit é **fail-open**: se o Redis cair, o login continua e o evento vai
para o log. Só habilite `trust_proxy_headers` atrás de um proxy que sobrescreve
`X-Forwarded-For`; conexão direta pode forjar o cabeçalho.

## Endpoints de infraestrutura

| Rota | O que faz |
|---|---|
| `GET /health` | liveness: o processo está de pé (não toca no banco) |
| `GET /health/ready` | readiness: exige Postgres e Redis; devolve `503` se algum cair |
| `GET /docs` | UI Scalar com todos os endpoints |
| `GET /api-docs/openapi.json` | especificação OpenAPI 3 |
| `GET /metrics` | métricas Prometheus (`axum_http_requests_*`) |

Erros saem em `application/problem+json` (RFC 9457), inclusive 404 e 405.
`/metrics` não tem autenticação: restrinja no proxy ou na rede.

## Autenticação e permissões

| Rota | O que faz |
|---|---|
| `POST /api/v1/auth/login` | e-mail + senha → access token (15 min) + refresh token (7 dias); `429` após 5 falhas na conta |
| `POST /api/v1/auth/refresh` | rotaciona o par; reutilizar um token consumido revoga a família inteira |
| `POST /api/v1/auth/logout` | revoga o refresh token (204) |
| `GET /api/v1/auth/me` | perfil do usuário autenticado (Bearer) |
| `PATCH /api/v1/auth/me` | altera o próprio nome (`profile:write`) |
| `PUT /api/v1/auth/me/password` | senha atual + nova; encerra todas as sessões (204) |

- Senha: **Argon2id**, mínimo de 12 caracteres.
- Access token: JWT HS256 com `sub`, `role`, `token_version`, `jti`, `iat`, `exp`, `iss`, `aud`.
- Refresh token: opaco, guardado **hasheado** no Redis, consumido por script atômico (get + delete + marcador de uso) e ligado a uma família. Reuso de token já consumido revoga todos os tokens daquela família.
- `token_version` sobe ao trocar senha/papel ou desativar: invalida refresh tokens e sessões sensíveis na hora.
- A troca da própria senha exige a senha atual, incrementa `token_version` e
  derruba **todas** as sessões, inclusive a que fez a troca; o login é refeito.
- Papéis: `admin` e `user`. Handlers checam **permissão**, nunca papel:

```rust
auth.require(USERS_WRITE)?;
```

| Papel | Permissões |
|---|---|
| `admin` | `users:read`, `users:write`, `users:delete`, `profile:write` |
| `user` | `profile:write` |

O extractor `AuthUser` é stateless; rotas sensíveis chamam
`users::service::require_session` para conferir `active`, `token_version` e papel no banco.

### Idempotência

`POST` aceita `Idempotency-Key` (até 128 caracteres ASCII). A primeira execução
grava a resposta por 24 h; repetir a mesma chave devolve a mesma resposta sem
repetir o efeito. Enquanto a primeira ainda está em andamento, a repetição
recebe `409`. A assinatura inclui o hash do `Authorization`, então a chave de um
usuário não reproduz a resposta de outro. Sem o cabeçalho, o comportamento é o
normal; Redis fora do ar também segue sem idempotência (fail-open).

## Gestão de usuários (admin)

| Rota | Permissão | O que faz |
|---|---|---|
| `GET /api/v1/users` | `users:read` | lista com `page`, `perPage`, `search`, `role`, `active` |
| `POST /api/v1/users` | `users:write` | cria usuário (não existe cadastro público) |
| `GET /api/v1/users/{id}` | `users:read` | detalhe |
| `PATCH /api/v1/users/{id}` | `users:write` | atualiza `name`, `email`, `role`, `active` |
| `DELETE /api/v1/users/{id}` | `users:delete` | soft delete (`active = false`), idempotente |
| `PUT /api/v1/users/{id}/password` | `users:write` | redefine a senha e invalida as sessões |

Proteções:

- admin não desativa nem rebaixa a si mesmo;
- não é possível remover o último admin ativo;
- mudar papel ou desativar incrementa `token_version` e derruba as sessões;
- `PATCH` usa `version` para concorrência otimista (409 se alguém alterou antes);
- `DELETE` repetido devolve `204` (idempotente).

## E-mail transacional (outbox)

O envio de e-mail nunca acontece no caminho da requisição. A intenção é gravada
na **mesma transação** da mudança de negócio, na tabela `email_outbox`:

```text
BEGIN
  INSERT INTO users ...
  INSERT INTO email_outbox (status = 'pending', dedupe_key = 'user-created:<id>')
COMMIT

worker (a cada 15 s)
  SELECT ... FOR UPDATE SKIP LOCKED
  -> envia
  -> sent | pending (retry) | failed
```

- **Não perde**: se o processo cair entre o commit e o envio, a linha continua
  `pending` e o worker tenta de novo.
- **Não duplica a linha**: `dedupe_key` é única por evento.
- **Entrega at-least-once**: o envio precisa ser idempotente do lado do provedor.
- **Retry**: 5 tentativas com backoff exponencial (1, 2, 4, 8, 16 min).
- **Limpeza**: e-mails `sent` com mais de 30 dias são removidos pelo cron diário.
- **Lease**: item em `processing` há mais de 5 min volta a ser elegível.

Eventos já ligados ao outbox: usuário criado (`users::service::create`, inclusive
pelo CLI) e senha alterada. O envio real fica atrás da trait `Mailer`
(`core::mail`), com implementações `log` e `smtp`.

> Um projeto novo deve enviar só o que é problema de negócio perder: convite,
> reset de senha, confirmação de pagamento. Métrica e log não precisam de outbox.

## Deploy (Docker)

A imagem é multi-stage (`rust:1.99.1-slim` → `debian:bookworm-slim`) e roda como
usuário `app` (UID 10001), sem privilégios:

```bash
just docker-build

APP__JWT__SECRET="$(openssl rand -base64 48)" \
  docker compose --profile app up -d --build

# migrations são explícitas
docker compose --profile tools run --rm migrate
```

- `api` publica `8080`; `worker` roda o outbox de e-mail e a limpeza.
- `migrate` fica no perfil `tools` e não roda no boot.
- `APP__SERVER__TRUST_PROXY_HEADERS` fica `false` por padrão; habilite só atrás
  de um proxy que sobrescreve `X-Forwarded-For`.
- `GET /metrics` deve ficar restrito à rede de monitoramento.

## Estrutura

```text
src/
├── main.rs                 # binário api
├── lib.rs
├── bin/
│   ├── worker.rs           # cron
│   ├── migrator.rs         # migrations
│   └── admin.rs            # bootstrap de admin
├── config/settings.rs      # figment + validação no boot
├── core/
│   ├── auth/               # role, permission, jwt, password, extractor
│   ├── cache.rs            # pool Redis + ping
│   ├── client_ip.rs        # IP do cliente + confiança no proxy
│   ├── error.rs            # AppError → Problem Details
│   ├── extract.rs          # ValidatedJson / ValidatedQuery
│   ├── idempotency.rs      # Idempotency-Key + replay no Redis
│   ├── mail.rs             # trait Mailer: log e smtp
│   ├── metrics.rs          # recorder Prometheus (OnceLock)
│   ├── openapi.rs          # schema do Problem
│   ├── response.rs         # Page<T> / PageQuery
│   ├── http.rs             # trace, cors, compress, timeout, request-id
│   └── health.rs           # liveness / readiness
├── database/
│   ├── connection.rs
│   └── entity/             # gerado pelo sea-orm-cli
├── migrations/             # in-crate (sea-orm-migration)
├── modules/
│   ├── auth/               # login, refresh, logout, me
│   ├── email/              # outbox + templates; dono da tabela email_outbox
│   └── users/              # dono da tabela users + CRUD admin
├── jobs/                   # scheduler do worker: outbox + limpeza
└── routes/
    ├── mod.rs              # monta health + módulos + docs
    └── openapi.rs          # /docs e /api-docs/openapi.json

tests/                      # integração com testcontainers
.github/workflows/ci.yml    # fmt, clippy, nextest, deny e docker build
Dockerfile                  # imagem multi-stage, non-root

docs/                       # o "porquê" (AGENTS.md é o contrato)
├── architecture.md
├── adding-a-module.md
├── security.md
├── testing.md
└── decisions.md
```

## Fases

| Fase | Entrega | Status |
|---|---|---|
| 0 | Esqueleto, config, compose, justfile | ok |
| 1 | Core: settings, state, erro, http, health, shutdown | ok |
| 2 | Migration `users`, entidade e migrator | ok |
| 3 | Auth JWT (login/refresh/logout/me) + bootstrap admin | ok |
| 4 | `users` como módulo-referência (CRUD admin) + OpenAPI | ok |
| 5 | Cache Redis no `users` | cancelada (ADR) |
| 6 | Worker + cron + e-mail por outbox | ok |
| 7 | Rate limit, reuso de refresh, métricas, Dockerfile, CI e snapshot OpenAPI | ok |

## Padrões

- IDs UUID v7; datas `timestamptz` UTC; dinheiro `numeric` + `rust_decimal`;
- JSON camelCase; listas `{ items, page, perPage, total }`;
- erros em `application/problem+json` (RFC 9457);
- rotas no plural sob `/api/v1`;
- migrations explícitas (`just migrate`), nunca no boot.
