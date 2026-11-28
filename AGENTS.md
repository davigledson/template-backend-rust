# AGENTS.md — contrato do template-backend-rust

Este arquivo vale para agentes de código e para pessoas. As regras são curtas,
verificáveis e não opcionais. O "porquê" completo está em `docs/`.

## Contexto

- Rust 1.99 (fixado em `rust-toolchain.toml`), edição 2024.
- Axum 0.8 + SeaORM 2.0 + PostgreSQL 18 + Redis 8.
- **Uma API** (`/api/v1`) + um **worker** de cron, do mesmo código.
- **Monólito modular por feature**: `modules/<feature>` é dono das suas rotas,
  regras e tabelas; `core` é infraestrutura compartilhada.
- Escopo atual: `users` + autenticação + e-mail transacional (outbox). Sem CRUD
  genérico e sem Keycloak.
- Alvo de deploy: 2 vCPU / 2 GB (API + Postgres + Redis no mesmo host).

## Antes de começar

1. Leia `docs/architecture.md` (desenho) e `docs/adding-a-module.md` (passo a passo).
2. Confira o ambiente: `just doctor`.
3. Nunca deixe servidor em foreground no chat. Use `just dev` em background ou o
   `davi-dev` do hub e leia só as últimas linhas do log.
4. Ao terminar, pare o processo que você iniciou.

## Comandos

| Comando | O que faz |
|---|---|
| `just setup` | `.env`, infra e dependências |
| `just rename <nome>` | renomeia o projeto a partir do template |
| `just doctor` | versões, containers e `.env` |
| `just up` / `down` / `reset` | infra (reset apaga volumes) |
| `just migrate` | aplica migrations pendentes |
| `just migration <nome>` | gera uma migration vazia e registra no `mod.rs` |
| `just module <nome>` | gera o esqueleto de um módulo (rota 501) |
| `just dev` | API em `http://localhost:8080` |
| `just worker` | worker de cron |
| `just mail-up` / `just mail-down` | Mailpit local (SMTP 1025, UI 8025) |
| `just test` | testes com `cargo nextest` |
| `just test-down` | derruba a infra compartilhada dos testes |
| `just fmt` / `just lint` / `just check` | qualidade |
| `just entities` | gera entidades a partir do banco |
| `just openapi-check` | snapshot do contrato OpenAPI |
| `just deny` | licenças, advisories e fontes |
| `just docker-build` | imagem de produção (`docker/Dockerfile`) |
| `just prod-migrate` | aplica migrations na stack de deploy |
| `just prod-admin <args>` | roda o CLI admin na stack de deploy |
| `just prod-up` | sobe infra + api + worker de deploy |
| `just prod-front` | sobe a SPA do front (perfil `front`, imagem em `FRONT_IMAGE`) |
| `just prod-down` | derruba a stack de deploy |
| `just prod-logs` | acompanha os logs da stack de deploy |
| `just smoke <url> [token]` | smoke pós-deploy de uma URL publicada |
| `just bench [duração] [conexões]` | benchmark dos endpoints principais (oha ou ab) |
| `just db-stats` | consultas mais caras e em execução no Postgres |
| `just admin-create <email> "<nome>" [true]` | cria admin (primeiro, ou outro com `true`) |
| `just admin-promote <email>` | promove usuário a admin |
| `just admin-reset-password <email>` | redefine senha e invalida sessões |
| `just admin-verify-email <email>` | confirma o e-mail de um usuário |
| `just admin-verify-email-all` | confirma todos os e-mails pendentes |
| `just outbox-list [status] [limit]` | lista e-mails do outbox |
| `just outbox-retry <id>` | reprocessa um e-mail na hora |
| `just outbox-retry-all [status] [force]` | reprocessa todos de um status |
| `just ci` | pipeline completo local |
| `just ci-fast` | fmt + clippy + check + testes unitários |
| `just ci-full` | fmt + clippy + nextest + deny |
| `just hooks` | instala hooks de pre-commit/pre-push |

## Mapa

```text
src/
├── main.rs            binário api
├── app.rs             estado + rotas + camadas HTTP
├── bin/               admin, migrator, worker
├── config/            settings/ (figment, uma seção por arquivo)
├── core/              auth, cache, client_ip, idempotency, mail, metrics,
│                      erro, http, response, health, startup, state
├── database/          connection + entity (gerada)
├── migrations/        migrations in-crate
├── modules/           auth, email, users
├── jobs/              scheduler do worker
└── routes/            só faz nest

docker/
├── Dockerfile         imagem multi-stage, non-root
├── compose.local.yml  Postgres + Redis + Mailpit de desenvolvimento
├── compose.deploy.yml api + worker + infra de deploy
└── compose.test.yml   Postgres/Redis compartilhados pelos testes
```

Direção das dependências:

```text
routes → modules/<feature> → core
                     ↘ database/entity (gerado)
```

## Regras que não se negociam

- **Só o módulo dono escreve na sua tabela.** Nada de SQL na tabela de outro módulo.
- **Sem ciclos.** `auth → users` pode; `users → auth` não.
- **`unwrap`, `expect` e `panic` são proibidos** no caminho de request. Erros viram
  `AppError` e saem em `application/problem+json`.
- **Permissão, não papel**: `auth.require(USERS_WRITE)`. Nunca `if role == admin`.
- **Entidade é gerada.** Rode `just entities`; não edite `database/entity/` à mão.
- **Migration é forward-only** e nunca roda no boot da API.
- **Segredo nunca** em TOML, log ou commit. Use `APP__*`/`.env` (só dev).
- **Validação no DTO**, antes do service (`ValidatedJson`/`ValidatedQuery` → 422
  em Problem Details). Repository não valida.
- **Transação curta**; efeito externo (e-mail, webhook) nunca dentro dela. Para
  e-mail, grave a intenção em `email_outbox` na mesma transação e deixe o worker
  enviar depois do commit.
- **Sem `utils.rs`.** Helper sem dono vai para `core::<tema>`; com dono, fica no módulo.
- **Sem repositório genérico** e sem container de DI: `AppState` + `Arc` bastam.
- **Sem emoji** em código, README, `AGENTS.md`, docs ou mensagens de commit.

## Camadas

| Camada | Pode | Não pode |
|---|---|---|
| `controller/` | extrair, validar via DTO, chamar service, responder | SQL, regra, Redis |
| `service/` | regra, transação, orquestrar repositórios | conhecer HTTP |
| `repository.rs` | query SeaORM (recebe a conexão) | validar, decidir status |
| `dto/` | serde + validação + schema | lógica |
| `entity/` | mapeamento gerado | ser editado à mão |
| `core/` | infraestrutura genérica | regra de feature |

`service.rs` é opcional em CRUD puro; regra ou transação exige service.

Alvos de tamanho: controller ≤120, service ≤150, repository ≤150, dto ≤80 linhas.
Passou disso, quebre por sub-recurso — não crie camada nova.

Configuração segue a mesma ideia: `config/settings/<chave>.rs` com o mesmo nome
da chave no TOML (`server`, `database`, `redis`, `rate_limit`, `jwt`, `mail`,
`jobs`, `log`, `cors`). Cada arquivo tem o tipo, o `Default`, os helpers
derivados e `collect_errors`; o `mod.rs` só monta o `Settings` e o agregado de
validação. Não engorde o `mod.rs`. `modules/users` é o
exemplo: `controller/{collection,item,password}.rs`,
`service/{lookup,create,update,deactivate,credentials,profile,session,rules}.rs`
e `dto/{list,output,write}.rs`, com o `mod.rs` só reexportando a API pública.

## Auth e permissões

- Papéis: `admin` e `user`. Permissões: `users:read`, `users:write`,
  `users:delete`, `profile:write`, `audit:read`.
- `AuthUser` (extractor) valida assinatura/expiração e **não consulta o banco**.
- Rotas sensíveis chamam `users::service::require_session`, que confere `active`,
  `token_version` e papel no banco.
- `token_version` sobe ao trocar senha/papel ou desativar → invalida refresh e
  sessões sensíveis na hora.
- Primeiro admin **só por CLI** (`just admin-create`). Nunca rota pública nem seed
  com senha.
- Access token: 15 min. O corpo do login/refresh devolve só ele; o cliente
  guarda em memória, nunca em `localStorage`.
- Refresh: opaco, hasheado no Redis, rotação atômica com marcador de uso
  (`GET`+`DEL`+`SET` em script). Vai apenas em cookie `HttpOnly`, com
  `Path=/api/v1/auth`, `SameSite` configurável e `Secure` em produção; nunca no
  corpo da resposta.
- `POST /auth/refresh` e `/auth/logout` leem o cookie. CORS usa credenciais e
  origens explícitas (`*` é recusado no boot); `Origin` que não seja o host da
  API nem esteja na lista recebe `403` nesses dois endpoints.
- Reuso de refresh já consumido revoga a **família** inteira; o token rotacionado
  daquela família também deixa de valer.
- Login tem rate limit por conta e por IP (Redis, janela fixa). Redis fora do ar
  é fail-open, com log; o proxy deve ter um limite por IP como segunda barreira.
- Perfil próprio: `PATCH /auth/me` (nome) e `PUT /auth/me/password` (senha atual
  + nova), ambos com `PROFILE_WRITE` e `require_session`.
- Troca de senha incrementa `token_version` e derruba **todas** as sessões; não
  emite par novo, o usuário faz login de novo.
- Recuperação de senha: `POST /auth/password/forgot` **sempre 204** (sem
  enumeração); código opaco de 30 min, hash no Redis, uso único e amarrado ao
  `token_version`. `POST /auth/password/reset` troca a senha e derruba sessões.
- Confirmação de e-mail: código de 7 dias, uso único, amarrado ao
  `token_version` e ao fingerprint do e-mail; trocar o e-mail zera
  `email_verified_at` e invalida o código anterior. O reenvio é público por
  e-mail (sempre 204, rate limit por conta/IP). `security.require_email_verified`
  (padrão `false`) transforma a confirmação em requisito de login. Em produção,
  `mail.provider = log` é recusado no boot, independentemente dessa flag.
- Sessões por dispositivo: uma entrada por família de refresh no Redis
  (`auth:session:<user>:<family>`), com IP e User-Agent. `GET/DELETE
  /auth/sessions` exige `require_session`; a sessão atual só sai pelo logout.
- Nunca logar código de recuperação/confirmação, refresh token ou senha.
- Auditoria: toda ação privilegiada (criar, editar, mudar papel, desativar,
  redefinir senha, confirmar e-mail pelo CLI) grava em `audit_log` **na mesma
  transação**, via `audit::service::record_user`. `changes` leva só o diff,
  nunca senha, hash ou token. `GET /audit` exige `audit:read` e confere a
  sessão no banco (ativo, `token_version` e papel); é somente leitura. O worker
  expurga o que passar de `audit.retention_days` (`0` mantém tudo).

## E-mail e jobs

- `modules/email` é dono de `email_outbox`; outro módulo chama
  `email::service::enqueue` **dentro da sua transação**, nunca envia direto.
- `dedupe_key` é única por evento (`user-created:<id>`); repetir não duplica.
- Envio fica atrás da trait `Mailer` em `core/mail/`: `log` (dev) ou `smtp`.
  Nunca logar corpo nem endereço completo no provider `log`.
- O worker (`src/jobs/`) só roda com `jobs.enabled = true`; job novo entra em
  `jobs::run`, com duração logada e sem segurar o tick.
- Entrega é at-least-once: o consumidor/provedor precisa tolerar duplicata.
- Limpeza de `sent` e `failed` antigos é job diário; a retenção de ambos é de
  30 dias. Não apague `pending`/`failed` manualmente sem inspeção.

## Banco e migrations

- UUID v7 (`Uuid::now_v7()`), `timestamptz` UTC, `numeric` para dinheiro.
- Tabelas e colunas em `snake_case`; tabela no plural.
- Toda FK e todo filtro/ordenação usado em query tem índice.
- Desativação por `active` (soft delete); `version` para concorrência otimista.
- Gere a migration com `just migration <nome>`; depois `just entities`.
- Nunca edite uma migration já aplicada; crie outra.

## API

- Prefixo `/api/v1`; rotas no plural; JSON em `camelCase`.
- Lista: `{ items, page, perPage, total }`.
- Erro: `application/problem+json` (RFC 9457).
- `404`/`405` também em Problem Details.
- Documentação: `/docs` (Scalar) e `/api-docs/openapi.json`. Todo handler novo tem
  `#[utoipa::path]` e entra no mesmo router via `routes!`.
- `POST` de recurso sob `/api/v1` **exige** `Idempotency-Key` (sem ela, `400`);
  retry nunca duplica. O middleware grava status, `content-type`, `location`,
  `etag` e corpo por 24 h. A assinatura é método + caminho + `sub` do access
  token + a chave; renovar o token não perde o replay. Repetição em andamento
  recebe `409`; `5xx` não é gravado.
- `/api/v1/auth/*` está fora da idempotência: a resposta carrega tokens e um
  replay devolveria refresh já rotacionado, disparando a revogação da família.
- `POST` que cria recurso devolve `Location` com a URL do recurso.
- Mudança breaking de contrato só com depreciação e aviso.

## Segurança

- Argon2id, mínimo de 12 caracteres; nunca logar senha, token ou PII.
- Limite de body, timeouts, CORS por ambiente (credenciais e origens exatas) e
  `x-request-id` já estão nas camadas.
- Respostas de `/api/v1` e `/metrics` levam `Cache-Control: no-store` e os
  cabeçalhos de segurança; HSTS só com `security.hsts_max_age_secs > 0`.
- `/docs` e `/api-docs` são desligados por `docs.enabled` (produção desliga);
  `/metrics` aceita `metrics.token` e deve ficar fora da internet.
- Rate limit de login, recuperação de senha e reenvio de confirmação; detecção
  de reuso de refresh. Ver decisões em `docs/decisions.md`.
- `X-Forwarded-For` só é confiável com `server.trust_proxy_headers = true`.
- Dependência nova: mantida, licença permitida, features mínimas, `cargo deny`.
- Segredo JWT: mínimo 32 caracteres, só via `APP__JWT__SECRET`.

## Performance (2 vCPU / 2 GB)

- Pool do banco: 8; Redis: 8; `worker_threads = 2` em produção.
- Proibido N+1; paginar sempre; sem `SELECT *` em rota quente.
- Sem I/O bloqueante dentro de `async`; sem transação ao redor de HTTP.
- Job de cron não pode segurar o tick; se passar de ~30 s, vira fila.
- Rate limit é fail-open por decisão: indisponibilidade de Redis não bloqueia login.
- Argon2 roda no máximo 2 tarefas por processo (`MAX_CONCURRENT_PASSWORD_TASKS`);
  cada uma usa ~19 MiB. Não remova o semáforo sem medir memória.
- Idempotência não guarda respostas acima de 256 KiB (a resposta passa, sem
  replay) para não consumir o orçamento de sessões/refresh no Redis.
- `/metrics` ligado exige `APP__METRICS__TOKEN` em produção; sem token o boot
  falha. Em desenvolvimento, sem token a rota fica pública.
- Meça antes de otimizar: `just bench` (oha/ab) com build release e
  `just db-stats` antes de mexer em pool, índice ou camada HTTP. Registre o
  resultado em `docs/capacity.md` com data, host e volume de dados.

## Testes

- Toda regra de service tem teste unitário.
- Todo endpoint novo tem teste de integração em
  `tests/api_flow.rs`; o outbox fica em `tests/email_flow.rs`, a confirmação
  de e-mail em `tests/email_verification.rs`, o hardening HTTP em
  `tests/hardening.rs` e a retenção/sessões em `tests/p2.rs`. Os helpers
  (harness, login, mailers, código no outbox) estão em
  `tests/common/mod.rs`.
- O setup script do `.config/nextest.toml` sobe `compose.test.yml` uma vez por
  execução; cada `Harness` recebe um banco no Postgres e um DB lógico no Redis,
  então não há estado compartilhado entre fluxos. Sem `cargo nextest`, o harness
  cai no testcontainers.
- Autorização e idempotência têm teste próprio.
- O snapshot do OpenAPI (`routes::openapi`) deve continuar verde; com mudança
  intencional de contrato, rode `INSTA_UPDATE=always cargo test --lib routes::openapi`
  e revise o diff.
- Rate limit, reuso de refresh e idempotência têm teste de integração próprio.
- Antes de finalizar: `just fmt && just lint && just test` + smoke real
  (`/health/ready` e, se mexeu em auth, login → me → refresh → logout com
  cookie jar: `curl -c`/`-b`).

## Novo módulo

1. `just module <nome>`: cria o esqueleto (controller/service/repository/dto),
   registra no router e gera a rota 501 no OpenAPI.
2. Migration da tabela (dono: o módulo).
3. `just migrate` e `just entities`.
4. `dto.rs` (input/output + validação).
5. `repository.rs` (queries).
6. `service.rs` (só se houver regra).
7. `controller.rs` (handlers + `router()`; troque o 501).
8. Permissões novas no catálogo, se houver.
9. Testes unitários + integração.
10. OpenAPI e `docs/` atualizados.

Se não couber em uma pasta, o módulo está grande demais.

## Definition of Done

- [ ] Migration + entidade gerada
- [ ] DTO com validação
- [ ] Repository com índice para os filtros usados
- [ ] Service com transação quando necessário
- [ ] Controller + rota + `router()`
- [ ] Permissão checada (se protegida)
- [ ] Idempotência definida (quando `POST`)
- [ ] Erros mapeados para Problem Details
- [ ] Testes unitários e de integração
- [ ] Sem `unwrap`, sem segredo em log
- [ ] `just ci` verde
- [ ] OpenAPI/docs atualizados

## Antipadrões

- `utils.rs` genérico; trait ou repositório genérico; container de DI.
- `anyhow` em handler; `unwrap`/`expect` em request.
- Middleware de auth onde o extractor resolve (mas use `require_session` em rota sensível).
- Editar entidade gerada ou migration já aplicada.
- SQL em tabela de outro módulo; ciclo entre módulos.
- Retry sem idempotência; cache invalidado antes do commit.
- Logar payload inteiro, token, senha ou dado pessoal.
- Rodar servidor em foreground no chat.

## Pare e pergunte

- Mudança de schema ou de migration aplicada.
- Dependência nova ou mudança de feature de crate.
- Mudança em auth, permissões, JWT ou bootstrap.
- Breaking change de API.
- Mudança de configuração/segredo, portas ou deploy.
- Exclusão de dados ou operação irreversível.

## Armadilhas conhecidas

- `jsonwebtoken` 12 exige a feature `rust_crypto` (já configurada).
- O refresh usa script Lua (GET + DEL + SET do marcador); Redis 8 é o alvo.
- `just entities` **sobrescreve** `src/database/entity/`.
- Projeto novo de cliente: `just rename <nome>` (ver `docs/rename.md`); não edite
  crate, imports, compose e tags à mão.
- Receitas do `just` não aceitam `:` no nome (`admin-create`, não `admin:create`).
- `.env` só é carregado no perfil `development`.
- `just prod-*` não carrega `.env`; exporte as variáveis ou use `APP_ENV_FILE`.
- API e worker validam a configuração; `migrator` e `admin` usam `load_unvalidated`.
- Portas do host: **55432** (Postgres) e **56379** (Redis).
- Entidade usa `DateTimeWithTimeZone`; converta para `Utc` no DTO.
- `email_outbox` usa `FOR UPDATE SKIP LOCKED`; o worker roda a cada 15 s e o
  lease de `processing` é 5 min.
- Redis é cache/fila de estado com `allkeys-lru` e sem AOF: **não** use como
  transporte durável de e-mail.
- `just worker` só processa algo com `jobs.enabled = true` e infra no ar.
- Para testar e-mail de verdade no dev: `just mail-up`, provider `smtp` em
  `smtp://127.0.0.1:1025` e UI em `http://localhost:8025`. Mailpit é só local;
  em produção use `smtps://` de um provedor real.
- O recorder Prometheus é global e instalado uma vez (`core::metrics`); não chame
  `PrometheusMetricLayer::pair()` direto em teste.
- Idempotência usa lock de 60 s e resposta de 24 h; o `Idempotency-Key` entra no
  hash, não no log. O Redis guarda só a resposta (sem tokens de sessão, porque
  `/auth/*` está fora).
- A imagem Docker não copia `tests/`; o build usa `--locked` e roda como UID 10001.

## Git

- **Mensagem de commit em português**, sempre, inclusive em repositório aberto.
- Formato Conventional Commits: `tipo(escopo): descrição no imperativo`.
  Tipos: `feat`, `fix`, `docs`, `refactor`, `perf`, `test`, `build`, `ci`,
  `chore`, `revert`. Escopo é o módulo/tema (`auth`, `audit`, `idempotency`,
  `config`, `deploy`) ou ausente quando for transversal.
- Descrição curta (até ~72 caracteres), minúscula, sem ponto final. Detalhe o
  porquê no corpo, também em português, quando o diff não for óbvio.
- Um assunto por commit: se a mensagem precisa de "e", provavelmente são dois.
- Não commitar `.env`, `target/`, segredo ou dado de teste.
- Histórico publicado não se reescreve para trocar idioma; corrija do commit
  seguinte em diante.
- Exemplos:
  `feat(audit): registra ações privilegiadas em log append-only`
  `fix(idempotency): exclui /auth do replay e assina pelo sub`
  `docs(operations): adiciona runbook do outbox`
- PR descreve contrato, migration, risco e como testar.

## Diagnóstico

- `GET /health` (liveness) e `GET /health/ready` (Postgres + Redis).
- `just doctor` mostra o estado da infra local; `just prod-up`, `just prod-migrate`
  e `just prod-logs` operam a stack de deploy.
- Logs: `just logs` (infra) e o log da API; nunca despeje o arquivo inteiro.
  Cada requisição tem um access log em `INFO` com `request_id`, método, caminho,
  status e `duration_ms`.
- Erro 500 só devolve mensagem genérica; o detalhe está no log com `x-request-id`.
- Query lenta: `just db-stats` (usa `pg_stat_statements`); o Postgres também
  grava no log as consultas acima de 500 ms.

## Links

- `docs/architecture.md` — desenho e decisões de arquitetura.
- `docs/adding-a-module.md` — tutorial passo a passo.
- `docs/security.md` — auth, e-mail, segredos e hardening.
- `docs/testing.md` — estratégia de testes.
- `docs/operations.md` — backup, outbox, métricas e incidentes.
- `deploy.md` — build, compose, migrations e checklist de produção.
- `docs/capacity.md` — orçamento de 2 vCPU / 2 GB e quando crescer.
- `docs/decisions.md` — registro de decisões (ADRs curtos).
