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
> [operação](docs/operations.md) · [capacidade](docs/capacity.md) ·
> [decisões](docs/decisions.md).

## Pré-requisitos (máquina do dev)

| Ferramenta | Obrigatória | Observação |
|---|---|---|
| `rustup` | sim | a versão do projeto está em `rust-toolchain.toml` |
| Docker + Compose | sim | sobe Postgres e Redis; não é preciso instalar banco local |
| `just` | sim | todos os comandos do projeto |
| `cargo-nextest` | recomendada | `just test` |
| `sea-orm-cli` 2.0.4 | só ao mexer no schema | `just entities` |
| `cargo-deny` | sim (CI) | licenças, advisories e fontes |

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
| `just mail-up` / `just mail-down` | Mailpit local: SMTP 1025 e UI 8025 |
| `just migrate` | migrations pendentes |
| `just admin-create <email> "<nome>" [true]` | cria o primeiro admin (ou outro com `true`) |
| `just admin-promote <email>` | promove usuário existente a admin |
| `just admin-reset-password <email>` | redefine a senha e invalida as sessões |
| `just admin-verify-email <email>` | confirma o e-mail de um usuário |
| `just admin-verify-email-all` | confirma todos os e-mails pendentes (backfill) |
| `just outbox-list [status] [limit]` | lista e-mails do outbox (padrão: `failed`, 20) |
| `just outbox-retry <id>` | devolve um e-mail ao processamento imediato |
| `just outbox-retry-all [status] [force]` | reprocessa todos de um status |
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

### E-mail local com Mailpit

Para ver o e-mail de verdade sem enviar para fora:

```bash
just mail-up

# .env
APP__MAIL__PROVIDER=smtp
APP__MAIL__SMTP_URL=smtp://127.0.0.1:1025
APP__MAIL__FROM=nao-responda@teste.local
```

Rode `just worker` para entregar o outbox e abra `http://localhost:8025`.
O Mailpit recebe qualquer destinatário, sem autenticação e sem TLS, e escuta
só em `127.0.0.1`. Com a API dentro do compose, use `smtp://mailpit:1025`.
Se as portas padrão estiverem ocupadas, defina `MAILPIT_SMTP_PORT` e
`MAILPIT_UI_PORT` no `.env` (o compose e o `just mail-up` respeitam).
Em produção o `mail.provider` continua `smtp` com `smtps://...` de um provedor
real; Mailpit não vai para produção.

Rate limit e proxy:

| Chave | Padrão | O que faz |
|---|---|---|
| `rate_limit.login_account_attempts` | 5 | falhas por conta na janela |
| `rate_limit.login_ip_attempts` | 20 | falhas por IP na janela |
| `rate_limit.login_window_secs` | 900 | janela fixa de 15 min |
| `rate_limit.password_reset_account_attempts` | 3 | pedidos de recuperação por conta |
| `rate_limit.password_reset_ip_attempts` | 10 | pedidos de recuperação por IP |
| `rate_limit.verification_account_attempts` | 3 | reenvios de confirmação por conta |
| `rate_limit.verification_ip_attempts` | 10 | reenvios de confirmação por IP |
| `security.require_email_verified` | `false` | exige e-mail confirmado para entrar |
| `security.hsts_max_age_secs` | 0 | HSTS em segundos; produção usa 31536000 |
| `docs.enabled` | `true` | expõe `/docs` e `/api-docs`; produção desliga |
| `metrics.enabled` | `true` | monta a rota `/metrics` |
| `metrics.token` | vazio | Bearer opcional do `/metrics` (mín. 16 caracteres) |
| `server.trust_proxy_headers` | `false` | lê `X-Forwarded-For`/`X-Real-IP` |

O rate limit é **fail-open**: se o Redis cair, o login continua e o evento vai
para o log. Só habilite `trust_proxy_headers` atrás de um proxy que sobrescreve
`X-Forwarded-For`; conexão direta pode forjar o cabeçalho.

## Endpoints de infraestrutura

| Rota | O que faz |
|---|---|
| `GET /health` | liveness: o processo está de pé (não toca no banco) |
| `GET /health/ready` | readiness: exige Postgres e Redis; devolve `503` se algum cair |
| `GET /docs` | UI Scalar com todos os endpoints, quando `docs.enabled = true` |
| `GET /api-docs/openapi.json` | especificação OpenAPI 3, quando `docs.enabled = true` |
| `GET /metrics` | Prometheus: `axum_http_requests_*` + domínio `app_*` (ver `docs/operations.md`) |

Erros saem em `application/problem+json` (RFC 9457), inclusive 404 e 405.
As respostas de `/api/v1` e `/metrics` levam `Cache-Control: no-store` e os
cabeçalhos de segurança (`nosniff`, `DENY`, `no-referrer`, `Permissions-Policy`).
HSTS só é enviado com `security.hsts_max_age_secs > 0`. Se `metrics.token`
estiver definido, `/metrics` exige `Authorization: Bearer <token>`; sem token,
restrinja a rota na rede/proxy.

## Autenticação e permissões

| Rota | O que faz |
|---|---|
| `POST /api/v1/auth/login` | e-mail + senha → access token (15 min) + refresh token (7 dias); `429` após 5 falhas na conta; `403` se o e-mail não estiver confirmado e a exigência estiver ligada |
| `POST /api/v1/auth/refresh` | rotaciona o par; reutilizar um token consumido revoga a família inteira |
| `POST /api/v1/auth/logout` | revoga o refresh token (204) |
| `GET /api/v1/auth/me` | perfil do usuário autenticado (Bearer) |
| `PATCH /api/v1/auth/me` | altera o próprio nome (`profile:write`) |
| `PUT /api/v1/auth/me/password` | senha atual + nova; encerra todas as sessões (204) |
| `POST /api/v1/auth/password/forgot` | envia código de recuperação; **sempre 204**, sem revelar se o e-mail existe |
| `POST /api/v1/auth/password/reset` | código + nova senha; encerra as sessões (204) |
| `POST /api/v1/auth/email/verify` | confirma o e-mail com o código recebido (204) |
| `POST /api/v1/auth/email/resend` | reenvia a confirmação por e-mail; **sempre 204**, sem revelar se a conta existe |
| `GET /api/v1/auth/sessions` | sessões ativas, com IP, dispositivo e `current` |
| `DELETE /api/v1/auth/sessions/{familyId}` | encerra outra sessão (204) |

- Senha: **Argon2id**, mínimo de 12 caracteres.
- Access token: JWT HS256 com `sub`, `role`, `token_version`, `fid` (sessão), `jti`, `iat`, `exp`, `iss`, `aud`.
- Refresh token: opaco, guardado **hasheado** no Redis, consumido por script atômico (get + delete + marcador de uso) e ligado a uma família. Reuso de token já consumido revoga todos os tokens daquela família.
- `token_version` sobe ao trocar senha/papel ou desativar: invalida refresh tokens e sessões sensíveis na hora.
- Recuperação de senha: código opaco de 30 min, guardado como hash no Redis,
  uso único e amarrado ao `token_version` (trocar a senha invalida códigos
  pendentes). O pedido devolve 204 mesmo para e-mail inexistente.
- Confirmação de e-mail: código de 7 dias, uso único, amarrado ao
  `token_version` e ao fingerprint do e-mail. Trocar o e-mail zera a
  confirmação e invalida o código anterior. O reenvio é público por e-mail
  (sempre 204) e limitado por conta/IP. Com
  `security.require_email_verified = true`, o login exige e-mail confirmado
  (403 até confirmar); o boot recusa essa combinação com `mail.provider = log`.
- Sessões: cada login é uma família de refresh com IP e User-Agent no Redis.
  `GET /auth/sessions` lista e `DELETE` revoga; a sessão atual só sai pelo
  logout. Se o Redis reinicia, as sessões (e o código de recuperação) caem junto.
- A troca da própria senha exige a senha atual, incrementa `token_version` e
  derruba **todas** as sessões, inclusive a que fez a troca; o login é refeito.
- Papéis: `admin` e `user`. Handlers checam **permissão**, nunca papel:

```rust
auth.require(USERS_WRITE)?;
```

| Papel | Permissões |
|---|---|
| `admin` | `users:read`, `users:write`, `users:delete`, `profile:write`, `audit:read` |
| `user` | `profile:write` |

O extractor `AuthUser` é stateless; rotas sensíveis chamam
`users::service::require_session` para conferir `active`, `token_version` e papel no banco.

### Idempotência

Todo `POST` de recurso sob `/api/v1` exige `Idempotency-Key` (até 128
caracteres ASCII); sem o cabeçalho a resposta é `400`. A primeira execução grava
status, `content-type`, `location`, `etag` e corpo por 24 h; repetir a mesma
chave devolve exatamente aquela resposta, sem repetir o efeito. Enquanto a
primeira ainda está em andamento, a repetição recebe `409`.

A assinatura é método + caminho + `sub` do access token + a chave. Como usa o
`sub` (e não o token inteiro), renovar o access token não perde o replay, e a
chave de um usuário não reproduz a resposta de outro. Resposta `5xx` não é
gravada; Redis fora do ar segue sem idempotência (fail-open).

`/api/v1/auth/*` fica **fora** da idempotência: a resposta carrega tokens e um
replay devolveria um refresh já rotacionado, o que dispararia a revogação da
família. Sessão não é uma operação de negócio idempotente.

```http
POST /api/v1/users
Idempotency-Key: 1c0a1f3f-6f2f-4a2b-9e0a-5c2b1a0d9e11
Authorization: Bearer <access>
Content-Type: application/json

201 Created
Location: /api/v1/users/01926f3a-...
```

## Gestão de usuários (admin)

| Rota | Permissão | O que faz |
|---|---|---|
| `GET /api/v1/users` | `users:read` | lista com `page`, `perPage`, `search`, `role`, `active` |
| `POST /api/v1/users` | `users:write` | cria usuário (não existe cadastro público); exige `Idempotency-Key` e devolve `Location` |
| `GET /api/v1/users/{id}` | `users:read` | detalhe |
| `PATCH /api/v1/users/{id}` | `users:write` | atualiza `name`, `email`, `role`, `active` |
| `DELETE /api/v1/users/{id}` | `users:delete` | soft delete (`active = false`), idempotente |
| `PUT /api/v1/users/{id}/password` | `users:write` | redefine a senha e invalida as sessões |
| `GET /api/v1/audit` | `audit:read` | histórico de ações privilegiadas, com filtros |

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
- **Limpeza**: o corpo é apagado logo depois do envio; a linha `sent` fica só
  com os metadados e é removida após 30 dias pelo cron diário.
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
- `GET /metrics` deve ficar restrito à rede de monitoramento; defina
  `APP__METRICS__TOKEN` para exigir Bearer.
- O perfil `production` desliga `/docs` e `/api-docs`; use
  `APP__DOCS__ENABLED=true` apenas se a documentação estiver protegida.

## Estrutura

```text
src/
├── main.rs                 # binário api
├── lib.rs
├── bin/
│   ├── worker.rs           # cron
│   ├── migrator.rs         # migrations
│   └── admin.rs            # bootstrap de admin
├── config/settings/        # uma seção por arquivo + validação no boot
├── core/
│   ├── auth/               # role, permission, jwt, password, extractor
│   ├── cache.rs            # pool Redis + ping
│   ├── client_ip.rs        # IP do cliente + confiança no proxy
│   ├── error.rs            # AppError → Problem Details
│   ├── extract.rs          # ValidatedJson / ValidatedQuery
│   ├── idempotency/        # Idempotency-Key + replay no Redis (key, store)
│   ├── mail/               # trait Mailer: log e smtp
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
├── common/mod.rs           # containers, login e mailers de teste
├── api_flow.rs             # auth, perfil, usuários, rate limit, idempotência
├── email_flow.rs           # outbox, dedupe, retry e limpeza do corpo
├── email_verification.rs   # reenvio público, troca de e-mail e flag de login
└── hardening.rs            # cabeçalhos, no-store, docs e métricas
.github/workflows/ci.yml    # fmt, clippy, nextest, deny e docker build
Dockerfile                  # imagem multi-stage, non-root

docs/                       # o "porquê" (AGENTS.md é o contrato)
├── architecture.md
├── adding-a-module.md
├── security.md
├── testing.md
├── operations.md
└── capacity.md
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
