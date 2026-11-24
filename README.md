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
just dev                      # API em http://localhost:8080
```

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

## Estrutura planejada

```text
src/
├── main.rs                 # binário api
├── lib.rs
├── bin/
│   ├── worker.rs           # cron
│   └── migrator.rs         # migrations (fase 2)
├── config/settings.rs      # figment + validação (fase 1)
├── core/                   # erro, response, state, http, cache, auth
├── database/
│   ├── connection.rs
│   └── entity/             # gerado pelo sea-orm-cli
├── modules/
│   ├── auth/
│   ├── users/
│   └── products/           # módulo-referência
├── jobs/                   # registro dos jobs (fase 6)
└── routes/                 # só faz nest dos routers
```

## Fases

| Fase | Entrega | Status |
|---|---|---|
| 0 | Esqueleto, config, compose, justfile | ✅ |
| 1 | Core: settings, state, erro, http, health, shutdown | ⬜ |
| 2 | Migrations, entidades e migrator | ⬜ |
| 3 | Auth JWT (login/refresh/logout/me) | ⬜ |
| 4 | `products` como módulo-referência + OpenAPI | ⬜ |
| 5 | Cache Redis | ⬜ |
| 6 | Worker + cron | ⬜ |
| 7 | Testes, métricas, Dockerfile, docs | ⬜ |

## Padrões

- IDs UUID v7; datas `timestamptz` UTC; dinheiro `numeric` + `rust_decimal`;
- JSON camelCase; listas `{ items, page, perPage, total }`;
- erros em `application/problem+json` (RFC 9457);
- rotas no plural sob `/api/v1`;
- migrations explícitas (`just migrate`), nunca no boot.
