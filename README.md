# template-backend-rust

Template de backend em **Rust** para projetos novos: Axum + SeaORM + PostgreSQL + Redis,
com JWT próprio, OpenAPI, cron em worker separado e configuração em camadas.

Arquitetura: **monólito modular por feature** — uma única API (`/api/v1`) + um worker,
módulos donos das suas rotas/regras/tabelas e um `core` fino de infraestrutura.

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
| `just worker` | processo de cron |
| `just migrate` | migrations pendentes |
| `just admin-create <email> "<nome>" [true]` | cria o primeiro admin (ou outro com `true`) |
| `just admin-promote <email>` | promove usuário existente a admin |
| `just admin-reset-password <email>` | redefine a senha e invalida as sessões |
| `just test` | testes unitários + integração |
| `just fmt` / `just lint` / `just check` | qualidade |
| `just entities` | gera entidades a partir do banco |
| `just ci` | pipeline local equivalente ao CI |

## Configuração

Precedência (do menor para o maior):

1. `default.toml` — padrões versionados;
2. `config/<perfil>.toml` — overrides por ambiente;
3. variáveis `APP__*` — `APP__DATABASE__URL` vira `database.url`;
4. `.env` — **apenas em desenvolvimento**, nunca versionado.

Segredos nunca ficam em TOML. `APP__JWT__SECRET` é obrigatório e precisa ter
no mínimo 32 caracteres (validado no boot).

## Endpoints de infraestrutura

| Rota | O que faz |
|---|---|
| `GET /health` | liveness: o processo está de pé (não toca no banco) |
| `GET /health/ready` | readiness: exige Postgres e Redis; devolve `503` se algum cair |

Erros saem em `application/problem+json` (RFC 9457), inclusive 404 e 405.

## Autenticação e permissões

| Rota | O que faz |
|---|---|
| `POST /api/v1/auth/login` | e-mail + senha → access token (15 min) + refresh token (7 dias) |
| `POST /api/v1/auth/refresh` | rotaciona o par; o refresh antigo deixa de valer |
| `POST /api/v1/auth/logout` | revoga o refresh token (204) |
| `GET /api/v1/auth/me` | perfil do usuário autenticado (Bearer) |

- Senha: **Argon2id**, mínimo de 12 caracteres.
- Access token: JWT HS256 com `sub`, `role`, `token_version`, `jti`, `iat`, `exp`, `iss`, `aud`.
- Refresh token: opaco, guardado **hasheado** no Redis e consumido com `GETDEL` (rotação atômica).
- `token_version` sobe ao trocar senha/papel ou desativar: invalida refresh tokens e sessões sensíveis na hora.
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

> Rate limit no login (`tower_governor`) entra na fase de hardening (7).

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
│   ├── error.rs            # AppError → Problem Details
│   ├── response.rs         # Page<T> / PageQuery
│   ├── http.rs             # trace, cors, compress, timeout, request-id
│   └── health.rs           # liveness / readiness
├── database/
│   ├── connection.rs
│   └── entity/             # gerado pelo sea-orm-cli
├── migrations/             # in-crate (sea-orm-migration)
├── modules/
│   ├── auth/               # login, refresh, logout, me
│   └── users/              # dono da tabela users
├── jobs/                   # registro dos jobs (fase 6)
└── routes/                 # só faz nest dos routers
```

## Fases

| Fase | Entrega | Status |
|---|---|---|
| 0 | Esqueleto, config, compose, justfile | ✅ |
| 1 | Core: settings, state, erro, http, health, shutdown | ✅ |
| 2 | Migration `users`, entidade e migrator | ✅ |
| 3 | Auth JWT (login/refresh/logout/me) + bootstrap admin | ✅ |
| 4 | `users` como módulo-referência (CRUD admin) + OpenAPI | ⬜ |
| 5 | Cache Redis no `users` | ⬜ |
| 6 | Worker + cron | ⬜ |
| 7 | Testes de integração, métricas, rate limit, Dockerfile, docs | ⬜ |

## Padrões

- IDs UUID v7; datas `timestamptz` UTC; dinheiro `numeric` + `rust_decimal`;
- JSON camelCase; listas `{ items, page, perPage, total }`;
- erros em `application/problem+json` (RFC 9457);
- rotas no plural sob `/api/v1`;
- migrations explícitas (`just migrate`), nunca no boot.
