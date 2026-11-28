# Template Backend Rust

Desenvolvido por **Davi Gledson da Silva Benedito** para projetos que precisam
de alto desempenho, baixo consumo de memória e uma base segura para autenticação,
usuários e e-mail transacional.

Stack: **Axum + SeaORM + PostgreSQL + Redis**, JWT HS256, OpenAPI, outbox de
e-mail, cron em worker separado e configuração em camadas.

Organização: **monólito modular por feature** — uma API (`/api/v1`) + um worker,
com rotas, regras e tabelas no módulo dono de cada domínio.

> **Vai mexer no código (pessoa ou agente)?** Leia o [`AGENTS.md`](AGENTS.md):
> regras, comandos e checklists.
>
> **Vai publicar?** O passo a passo de produção está em [`deploy.md`](deploy.md).
>
> Guias: [arquitetura](docs/architecture.md) ·
> [adicionar módulo](docs/adding-a-module.md) ·
> [segurança](docs/security.md) ·
> [testes](docs/testing.md) ·
> [operação](docs/operations.md) · [capacidade](docs/capacity.md).

## O que vem pronto

- **Autenticação:** login, refresh rotativo com detecção de reuso, logout,
  perfil, troca de senha e sessões por dispositivo.
- **Usuários:** CRUD administrativo com papéis, concorrência otimista e
  proteção contra remover o último admin.
- **E-mail:** recuperação de senha e confirmação de e-mail por código opaco,
  entregues por **outbox transacional**.
- **Auditoria:** ações privilegiadas registradas na mesma transação da mudança.
- **Proteções:** Argon2id, rate limit por conta/IP, `Idempotency-Key`, erros
  RFC 9457 e cabeçalhos de segurança.
- **Operação:** health checks, métricas Prometheus, OpenAPI, worker de cron e
  CLI de manutenção.
- **Qualidade:** testes unitários e de integração, snapshot de contrato, CI e
  hooks locais.

Para adicionar outro domínio, crie um módulo seguindo
[`docs/adding-a-module.md`](docs/adding-a-module.md).

## Pré-requisitos (máquina do dev)

| Ferramenta | Obrigatória | Observação |
|---|---|---|
| `rustup` | sim | a versão está em `rust-toolchain.toml` |
| Docker + Compose | sim | sobe Postgres, Redis e Mailpit local |
| `just` | sim | todos os comandos do projeto |
| `lld` | sim | linker configurado em `.cargo/config.toml` |
| `cargo-nextest` | recomendada | `just test`, `just ci-fast` (≥ 0.9.98 para setup scripts) |
| `sea-orm-cli` 2.0.4 | só ao mexer no schema | `just entities` |
| `cargo-deny` | recomendada | `just deny`, CI |

```bash
# Arch/CachyOS
sudo pacman -S --needed base-devel pkg-config lld just cargo-nextest cargo-deny
cargo install sea-orm-cli --version 2.0.4 --locked
```

## Começando

```bash
cp .env.example .env          # preencha APP__JWT__SECRET
just setup                    # .env + infra + dependências
just migrate                  # aplica as migrations
just admin-create admin@empresa.com "Admin"   # primeiro admin
just dev                      # API em http://localhost:8080
```

Crie o primeiro admin pelo CLI. Em automação, use:

```bash
ADMIN_PASSWORD='uma-senha-bem-longa-123' just admin-create admin@empresa.com "Admin"
```

As portas do host são configuráveis no `.env` (`POSTGRES_PORT`, `REDIS_PORT`) e
por padrão usam **55432** e **56379**, para não colidir com outros projetos que
já ocupam 5432/6379.

### Projeto novo a partir do template

```bash
just rename seu-projeto --dry-run   # mostra o que mudaria
just rename seu-projeto             # pede confirmação
```

Troca crate, imports, compose, tags, OpenAPI e issuer/audience do JWT; a prosa
que fala “template” fica. Detalhes e checklist em
[`docs/rename.md`](docs/rename.md).

## Escolha o recurso certo

| Necessidade | Use | Onde |
|---|---|---|
| Login, token, refresh, sessão | módulo `auth` | `src/modules/auth` |
| Criar/editar/desativar usuário e papel | módulo `users` | `src/modules/users` |
| Enviar e-mail | outbox do módulo `email` | `email::service::enqueue` na transação |
| Registrar ação privilegiada | auditoria | `audit::service::record_user` na transação |
| Evitar duplicata em retry de `POST` | `Idempotency-Key` | cabeçalho HTTP |
| Limitar abuso | rate limit | `[rate_limit]` em `default.toml` |
| Cron, outbox e limpeza | worker | `src/jobs` |
| Bootstrap e manutenção | CLI `admin` | `just admin-*` |
| Contrato da API | OpenAPI | `#[utoipa::path]` + `/docs` |
| Métricas | Prometheus | `core/metrics` + `/metrics` |
| Nova tabela | migration | `src/migrations` + `just migrate` |

Regras práticas:

- **Nunca envie e-mail direto no handler.** Grave no outbox na mesma transação
  da mudança de negócio; o worker entrega.
- **Nunca faça bootstrap de admin por rota pública.** Use o CLI.
- **Nunca rode migration no boot da API.** O deploy aplica antes de subir.
- **Nunca coloque segredo em TOML.** Use ambiente/secret manager.
- **Use o Redis para** sessão, rate limit, idempotência e códigos opacos.

## Comandos

| Comando | Faz |
|---|---|
| `just setup` | prepara `.env`, sobe a infra e baixa dependências |
| `just rename <nome>` | renomeia o projeto a partir do template |
| `just doctor` | versões, estado dos containers e do `.env` |
| `just up` / `just down` / `just reset` | infra local (reset apaga volumes) |
| `just docker-build` | build da imagem em `docker/Dockerfile` |
| `just prod-migrate` / `just prod-up` | stack de deploy: migrations e aplicação |
| `just prod-admin <args>` | CLI admin na stack de deploy |
| `just prod-down` / `just prod-logs` | derruba ou acompanha a stack de deploy |
| `just smoke <url> [token]` | smoke pós-deploy (health, cabeçalhos, login e métricas) |
| `just bench [duração] [conexões]` | benchmark dos endpoints principais (oha ou ab) |
| `just db-stats` | consultas mais caras e em execução no Postgres |
| `just dev` | API com logs legíveis |
| `just worker` | entrega do outbox e limpeza diária (cron) |
| `just mail-up` / `just mail-down` | Mailpit local: SMTP 1025 e UI 8025 |
| `just migrate` | migrations pendentes |
| `just migration <nome>` | gera uma migration vazia e registra no `mod.rs` |
| `just module <nome>` | gera o esqueleto de um módulo (rota 501 + OpenAPI) |
| `just admin-create <email> "<nome>" [true]` | cria o primeiro admin (ou outro com `true`) |
| `just admin-promote <email>` | promove usuário existente a admin |
| `just admin-reset-password <email>` | redefine a senha e invalida as sessões |
| `just admin-verify-email <email>` | confirma o e-mail de um usuário |
| `just admin-verify-email-all` | confirma todos os e-mails pendentes (backfill) |
| `just outbox-list [status] [limit]` | lista e-mails do outbox (padrão: `failed`, 20) |
| `just outbox-retry <id>` | devolve um e-mail ao processamento imediato |
| `just outbox-retry-all [status] [force]` | reprocessa todos de um status |
| `just test` | testes unitários + integração |
| `just test-down` | derruba a infra compartilhada dos testes |
| `just fmt` / `just lint` / `just check` | qualidade |
| `just openapi-check` | snapshot do contrato OpenAPI |
| `just deny` | licenças, advisories e fontes das dependências |
| `just entities` | gera entidades a partir do banco |
| `just ci` | pipeline completo local |
| `just ci-fast` | feedback rápido: fmt + clippy + check + testes unitários |
| `just ci-full` | verificação completa antes de push |
| `just hooks` | instala hooks de pre-commit/pre-push neste clone |

## Configuração

### Onde cada coisa mora

Precedência (do menor para o maior):

1. `default.toml` — padrões versionados e seguros;
2. `config/<perfil>.toml` — overrides por ambiente;
3. variáveis `APP__*` — `APP__DATABASE__URL` vira `database.url`.

O `.env` não é uma camada acima: em `development` ele só preenche variáveis que
ainda não estão no ambiente. Uma variável exportada no shell vence o `.env`.
Em produção, a aplicação nem lê o arquivo.

| Lugar | Use para | Versionar? |
|---|---|---|
| `default.toml` | padrões que valem para todos | sim |
| `config/development.toml` | logs, CORS local, ajustes de dev | sim |
| `config/production.toml` | CORS, docs, HSTS, ajustes de produção | sim |
| `.env` | segredos e portas da sua máquina | **não** |
| ambiente do deploy | segredos e URLs reais (`APP__*`) | **não** |

`APP_PROFILE` aceita `development`, `production` ou um nome com arquivo em
`config/<perfil>.toml`; perfil desconhecido falha no boot. Em produção, injete as
variáveis pelo ambiente, Docker secrets ou secret manager do provedor.

### Nome das variáveis

O mapeamento é por `__`:

```text
APP__DATABASE__URL      -> [database] url
APP__MAIL__SMTP_URL     -> [mail] smtp_url
APP__RATE_LIMIT__LOGIN_ACCOUNT_ATTEMPTS -> [rate_limit] login_account_attempts
```

Segredos nunca entram em TOML nem no Git. `APP__JWT__SECRET` é obrigatório,
precisa ter **no mínimo 32 caracteres** e é validado no boot.

### Núcleo

| Variável | Padrão | O que faz |
|---|---|---|
| `APP_PROFILE` | `development` | perfil ativo (`development`/`production`) |
| `APP__JWT__SECRET` | — | segredo HS256, mínimo 32 caracteres |
| `APP__DATABASE__URL` | `postgres://app:app@localhost:55432/app` | conexão Postgres |
| `APP__REDIS__URL` | `redis://localhost:56379` | conexão Redis |
| `APP__SERVER__TRUST_PROXY_HEADERS` | `false` | confia em `X-Forwarded-For`/`X-Real-IP` |
| `APP__JOBS__ENABLED` | `true` | liga o scheduler do worker |
| `APP__JOBS__TIMEZONE` | `America/Sao_Paulo` | fuso dos jobs |
| `APP__LOG__LEVEL` | `info` | nível de log (`debug` em dev) |
| `RUST_LOG` | — | override fino de logs do `tracing` |

`server.trust_proxy_headers` só deve ser `true` atrás de um proxy que define
`X-Real-IP` ou acrescenta o cliente no fim do `X-Forwarded-For`. Conexão direta
permite forjar o cabeçalho.

### E-mail

| Chave | Padrão | O que faz |
|---|---|---|
| `mail.provider` | `log` | `log` registra sem enviar (só dev); `smtp` envia de verdade. Em produção, `log` é recusado no boot |
| `mail.from` | `nao-responda@example.com` | remetente |
| `mail.smtp_url` | vazio | obrigatório com `provider = smtp` |

Em desenvolvimento use `log`. Para ver o e-mail de verdade sem sair da máquina:

```bash
just mail-up

# .env
APP__MAIL__PROVIDER=smtp
APP__MAIL__SMTP_URL=smtp://127.0.0.1:1025
APP__MAIL__FROM=nao-responda@teste.local
```

Rode `just worker` para entregar o outbox e abra `http://localhost:8025`.
O Mailpit escuta só em `127.0.0.1`, sem TLS nem autenticação. Se as portas
padrão estiverem ocupadas, defina `MAILPIT_SMTP_PORT` e `MAILPIT_UI_PORT` no
`.env`. Em produção, use `smtps://...` de um provedor real; Mailpit não vai para
produção.

### Segurança, limites e observabilidade

| Chave | Padrão | O que faz |
|---|---|---|
| `rate_limit.login_account_attempts` | 5 | falhas de login por conta na janela |
| `rate_limit.login_ip_attempts` | 20 | falhas de login por IP na janela |
| `rate_limit.login_window_secs` | 900 | janela fixa de 15 min |
| `rate_limit.password_reset_account_attempts` | 3 | pedidos de recuperação por conta |
| `rate_limit.password_reset_ip_attempts` | 10 | pedidos de recuperação por IP |
| `rate_limit.verification_account_attempts` | 3 | reenvios de confirmação por conta |
| `rate_limit.verification_ip_attempts` | 10 | reenvios de confirmação por IP |
| `security.require_email_verified` | `false` | exige e-mail confirmado para entrar |
| `security.hsts_max_age_secs` | 0 | HSTS; só faz efeito em HTTPS |
| `database.statement_timeout_secs` | 5 | limite por statement no pool da API; `0` desliga |
| `docs.enabled` | `true` | expõe `/docs` e `/api-docs` |
| `metrics.enabled` | `true` | monta a rota `/metrics`; em produção exige token ou desligar |
| `metrics.token` | vazio | Bearer do `/metrics` (mín. 16); obrigatório em produção com a rota ligada |
| `audit.retention_days` | 365 | retenção do `audit_log`; `0` mantém tudo |

O rate limit é **fail-open**: se o Redis cair, o login continua e o evento vai
para o log. A segunda barreira é o limite por IP no proxy.

As respostas de `/api/v1` e `/metrics` levam `Cache-Control: no-store` e os
cabeçalhos `nosniff`, `DENY`, `no-referrer` e `Permissions-Policy`. HSTS só é
enviado com `security.hsts_max_age_secs > 0`. Se `metrics.token` estiver
definido, `/metrics` exige `Authorization: Bearer <token>`; em produção, a rota
ligada sem token impede o boot.

Cada requisição gera um access log em `INFO` com `request_id`, método, caminho,
status e `duration_ms`. Cabeçalhos (`authorization`, `cookie`) e query string
não entram no log. Em produção o `x-request-id` aparece em todas as linhas do
JSON; use-o para seguir uma requisição.

## Endpoints de infraestrutura

| Rota | O que faz |
|---|---|
| `GET /health` | liveness: o processo está de pé (não toca no banco) |
| `GET /health/ready` | readiness: exige Postgres e Redis |
| `GET /docs` | UI Scalar, quando `docs.enabled = true` |
| `GET /api-docs/openapi.json` | contrato OpenAPI 3, quando `docs.enabled = true` |
| `GET /metrics` | Prometheus: `axum_http_requests_*` + métricas `app_*` |

Erros saem em `application/problem+json` (RFC 9457), inclusive 404 e 405.

## Autenticação e permissões

| Rota | O que faz |
|---|---|
| `POST /api/v1/auth/login` | e-mail + senha → access token (15 min) + refresh token (7 dias); `429` após 5 falhas na conta; `403` se a exigência de e-mail confirmado estiver ligada |
| `POST /api/v1/auth/refresh` | rotaciona o par; reutilizar um token consumido revoga a família inteira |
| `POST /api/v1/auth/logout` | revoga o refresh token (204) |
| `GET /api/v1/auth/me` | perfil do usuário autenticado (Bearer) |
| `PATCH /api/v1/auth/me` | altera o próprio nome (`profile:write`) |
| `PUT /api/v1/auth/me/password` | senha atual + nova; encerra todas as sessões (204) |
| `POST /api/v1/auth/password/forgot` | envia código de recuperação; **sempre 204** |
| `POST /api/v1/auth/password/reset` | código + nova senha; encerra as sessões (204) |
| `POST /api/v1/auth/email/verify` | confirma o e-mail com o código recebido (204) |
| `POST /api/v1/auth/email/resend` | reenvia a confirmação; **sempre 204** |
| `GET /api/v1/auth/sessions` | sessões ativas, com IP, dispositivo e `current` |
| `DELETE /api/v1/auth/sessions/{familyId}` | encerra outra sessão (204) |

- Senha: **Argon2id**, mínimo de 12 caracteres.
- Access token: JWT HS256 de 15 min; o refresh rotaciona o par.
- Refresh token: opaco, uso único e ligado a uma família. Reutilizar um token
  consumido revoga a família inteira.
- Trocar senha, papel ou desativar o usuário invalida tokens e sessões na hora.
- Recuperação de senha: código de 30 min, uso único e amarrado ao e-mail. O
  pedido devolve 204 mesmo para e-mail inexistente.
- Confirmação de e-mail: código de 7 dias, uso único e amarrado ao e-mail.
  Trocar o e-mail invalida o código anterior; o reenvio é público (sempre 204) e
  limitado por conta/IP.
- Sessões: cada login é uma sessão com IP e User-Agent no Redis.
  `GET /auth/sessions` lista e `DELETE` revoga; a sessão atual só sai pelo
  logout. Se o Redis reinicia, sessões e códigos caem junto.
- A troca da própria senha exige a senha atual e derruba **todas** as sessões,
  inclusive a que fez a troca.

| Papel | Permissões |
|---|---|
| `admin` | `users:read`, `users:write`, `users:delete`, `profile:write`, `audit:read` |
| `user` | `profile:write` |

Handlers checam **permissão**, nunca papel:

```rust
auth.require(USERS_WRITE)?;
```

Rotas sensíveis conferem a sessão no banco (`require_session`): usuário ativo,
`token_version` atual e papel válido. Isso vale para `/auth/me`, gestão de
usuários e `/audit`.

## Gestão de usuários (admin)

| Rota | Permissão | O que faz |
|---|---|---|
| `GET /api/v1/users` | `users:read` | lista com `page`, `perPage`, `search`, `role`, `active` |
| `POST /api/v1/users` | `users:write` | cria usuário; exige `Idempotency-Key` e devolve `Location` |
| `GET /api/v1/users/{id}` | `users:read` | detalhe |
| `PATCH /api/v1/users/{id}` | `users:write` | atualiza `name`, `email`, `role`, `active`; aceita `version` |
| `DELETE /api/v1/users/{id}` | `users:delete` | soft delete (`active = false`), idempotente |
| `PUT /api/v1/users/{id}/password` | `users:write` | redefine a senha e invalida as sessões |
| `GET /api/v1/audit` | `audit:read` | histórico de ações privilegiadas, com filtros |

Proteções:

- admin não desativa nem rebaixa a si mesmo;
- não é possível remover o último admin ativo;
- mudar papel ou desativar derruba as sessões;
- `PATCH` aceita a `version` devolvida por `GET /users/{id}`; se ela já mudou,
  devolve `409`. Sem `version`, o servidor usa a versão lida na transação e só
  detecta escritas simultâneas;
- `DELETE` repetido devolve `204`.

## Idempotência

Use `Idempotency-Key` quando um `POST` cria recurso ou tem efeito externo e o
cliente pode tentar de novo após timeout/perda de resposta. A chave evita
duplicar o efeito.

```http
POST /api/v1/users
Idempotency-Key: 1c0a1f3f-6f2f-4a2b-9e0a-5c2b1a0d9e11
Authorization: Bearer <access>
Content-Type: application/json

201 Created
Location: /api/v1/users/01926f3a-...
```

Como usar:

- gere **uma chave por operação lógica** (UUID é uma boa escolha);
- reutilize a mesma chave **somente no retry** daquela operação;
- se mudar o corpo, gere outra chave;
- não use em `/api/v1/auth/*`: login/refresh/logout carregam tokens e ficam fora
  do replay.

Todo `POST` de recurso sob `/api/v1` exige a chave (até 128 caracteres ASCII
visíveis). A resposta fica 24 h no Redis; repetir a mesma chave devolve status,
`content-type`, `location`, `etag` e corpo originais. Enquanto a primeira
execução está em andamento, a repetição recebe `409`. Resposta `5xx` não é
gravada; Redis fora do ar segue sem idempotência (fail-open).

## E-mail transacional (outbox)

O envio nunca acontece no caminho da requisição. A intenção é gravada na
**mesma transação** da mudança de negócio:

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

- **Queda entre commit e envio:** a linha continua `pending` e o worker tenta de
  novo.
- **Deduplicação:** `dedupe_key` é única por evento.
- **Entrega at-least-once:** o envio precisa ser idempotente no provedor.
- **Retry:** 5 tentativas com backoff exponencial (1, 2, 4, 8, 16 min).
- **Limpeza:** o corpo é apagado após o envio; a linha `sent` guarda só
  metadados e é removida após 30 dias.
- **Lease:** item em `processing` há mais de 5 min volta a ser elegível.

Eventos já ligados ao outbox: usuário criado e senha alterada. O envio real fica
atrás da trait `Mailer` (`core::mail`), com implementações `log` e `smtp`.

## Estrutura

```text
src/
├── main.rs                 # binário api
├── lib.rs
├── bin/
│   ├── worker.rs           # cron
│   ├── migrator.rs         # migrations
│   └── admin.rs            # bootstrap e manutenção
├── config/settings/        # uma seção por arquivo + validação no boot
├── core/
│   ├── auth/               # role, permission, jwt, password, extractor
│   ├── idempotency/        # Idempotency-Key + replay no Redis
│   ├── mail/               # trait Mailer: log e smtp
│   ├── metrics.rs          # recorder Prometheus
│   ├── http.rs             # camadas HTTP e cabeçalhos
│   └── ...                 # cache, client_ip, error, extract, response, health
├── database/               # connection + entity gerada
├── migrations/             # migrations in-crate
├── modules/
│   ├── auth/               # login, refresh, logout, perfil e sessões
│   ├── email/              # outbox + templates; dono de email_outbox
│   └── users/              # dono de users + CRUD admin
├── jobs/                   # scheduler do worker: outbox + limpeza
└── routes/                 # monta health + módulos + docs

docker/
├── Dockerfile              # imagem de deploy: api, worker e migrator
├── compose.local.yml       # Postgres + Redis + Mailpit de desenvolvimento
├── compose.deploy.yml      # infra + api + worker de deploy
└── compose.test.yml        # Postgres/Redis compartilhados pelos testes

tests/                      # integração com Postgres/Redis reais
├── common/mod.rs           # harness, login, mailers e helpers
├── admin_invariant.rs      # concorrência do último admin
├── api_flow.rs             # auth, perfil, usuários, rate limit, idempotência
├── auth_recovery_flow.rs   # reset de senha, confirmação e sessões
├── email_flow.rs           # outbox, dedupe, retry e limpeza do corpo
├── email_verification.rs   # reenvio público, troca de e-mail e flag
├── hardening.rs            # cabeçalhos, no-store, docs e métricas
└── p2.rs                   # retenção, sessões e código anterior

.github/workflows/ci.yml    # quality em PR e push (feedback de desenvolvimento)
.github/workflows/production.yml # build e smoke de runtime, manual (tag em produção)
.github/workflows/smoke.yml # smoke pós-deploy contra BASE_URL
.config/nextest.toml        # retries, timeouts e setup da infra de teste
scripts/                    # hooks, infra de teste e rename do projeto
docs/                       # arquitetura, segurança, testes, operação e rename
deploy.md                   # build, compose, migrations e checklist de produção
```

## Convenções da API e do banco

- IDs UUID v7; datas `timestamptz` UTC; dinheiro `numeric` + `rust_decimal`;
- JSON camelCase; listas `{ items, page, perPage, total }`;
- erros em `application/problem+json` (RFC 9457);
- rotas no plural sob `/api/v1`;
- migrations explícitas (`just migrate`), nunca no boot.

## Próximos passos

- Para criar um módulo novo, siga [`docs/adding-a-module.md`](docs/adding-a-module.md).
- Para publicar, use [`deploy.md`](deploy.md).
