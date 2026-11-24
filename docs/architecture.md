# Arquitetura

Este documento explica o **porquê** do desenho. As regras operacionais estão no
[`AGENTS.md`](../AGENTS.md).

## Visão geral

```text
cliente HTTP ──► api (Axum) ──► modules/<feature> ──► core (infra)
                                    │
                                    ├──► PostgreSQL (SeaORM)
                                    └──► Redis (refresh tokens, rate limit)

cron ─────────► worker (mesmo código, sem HTTP) ──► email_outbox ──► SMTP
monitor ──────► /metrics (Prometheus, sem auth; restrinja no proxy)
```

- **Um binário de API** expõe tudo em `/api/v1`.
- **Um binário de worker** roda os jobs de cron, com o mesmo `core` e os mesmos
  módulos, mas sem subir servidor HTTP.
- **Dois binários auxiliares**: `migrator` (schema) e `admin` (bootstrap).
- Postgres e Redis ficam no mesmo host; no dev, em containers.

Um processo separado para cron evita que um job longo segure uma réplica da API
e permite escalar API e worker de forma independente.

## Monólito modular

O sistema é um monólito, mas organizado por **feature**, não por camada técnica:

```text
src/modules/auth/      dono de login/refresh/logout/me
src/modules/users/     dono da tabela users
src/modules/email/     dono da tabela email_outbox e dos templates
src/core/              infraestrutura compartilhada
```

Cada módulo tem as mesmas peças:

| Arquivo | Papel |
|---|---|
| `mod.rs` | expõe o `router()` e a API pública do módulo |
| `controller.rs` | handlers HTTP, extração e resposta |
| `dto.rs` | contratos de entrada/saída + validação |
| `service.rs` | regra de negócio e transação |
| `repository.rs` | queries SeaORM |

Um módulo é dono das suas tabelas. Se outro módulo precisa de dados, chama o
**service** do dono, nunca o repositório nem SQL direto.

Exemplo real: `auth` precisa autenticar um usuário. Ele chama
`users::service::find_by_email` — não conhece a tabela `users`.

### Por que não camadas globais

`controllers/`, `services/` e `repositories/` globais espalham uma feature por
três árvores de diretório: para entender "login" você abre seis arquivos em
pastas distantes. Por feature, tudo que muda junto fica junto e o limite do
módulo é o limite do contexto.

### Quando dividir

- Módulo passou de ~800 linhas → divida por sub-recurso (`users/invites`,
  `billing/invoices`).
- Compile passou de ~2 min → workspace com crates por domínio.
- Necessidade real de escala ou deploy independente → extrair serviço. Só então.

## Direção das dependências

```text
routes ──► modules/<feature> ──► core
                  │
                  └────────────► database/entity (gerado)
```

Proibido: `core` importar `modules`, um módulo importar o controller de outro, e
ciclos (`auth → users` pode; `users → auth` não).

O `AppState` carrega `db`, `cache`, `jwt`, `mailer` e `config`; os serviços recebem
só o que precisam (`&state.db`, `&state.cache`), o que os deixa testáveis sem
subir a aplicação inteira.

## Ciclo de uma requisição

```text
TCP
 └─ camadas (core/http.rs): request-id → trace → cors → compress → timeout → body limit
     └─ IP do cliente (core/client_ip.rs) → métricas → idempotência
         └─ router /api/v1
             └─ extractor (AuthUser, ClientIp, Json, Path, Query, Valid)
             └─ controller
                 ├─ service (regra, transação)
                 │   └─ repository (SeaORM) ──► PostgreSQL / Redis
                 └─ resposta JSON ou Problem Details
```

- O **extractor** valida autenticação e devolve `AuthUser` tipado.
- O **controller** só orquestra: não faz SQL, não abre Redis, não decide regra.
- O **service** não conhece `StatusCode` nem `Json`; ele devolve domínio ou
  `AppError`.
- O **erro** sempre termina em `application/problem+json` (RFC 9457) e 5xx nunca
  vaza detalhe para o cliente: o detalhe fica no log com o `x-request-id`.

A documentação não é um artefato paralelo: cada handler declara
`#[utoipa::path]` e é registrado por `routes!`, então o mesmo router atende e
gera o OpenAPI. `routes/openapi.rs` serve o JSON em `/api-docs/openapi.json` e o
Scalar em `/docs`.

## Configuração

Ordem de precedência (a última vence):

```text
default.toml → config/<perfil>.toml → variáveis APP__* → .env (só em development)
```

- O perfil vem de `APP_PROFILE` (padrão `development`).
- O `.env` **só** é carregado em `development`; em produção tudo vem do ambiente.
- `APP__JWT__SECRET` não tem valor padrão e é obrigatório; o boot falha se tiver
  menos de 32 caracteres.
- `Settings::load` valida (timeouts, pool, timezone, segredo). `migrator` e
  `admin` usam `load_unvalidated` porque não sobem servidor e não precisam de
  tudo válido.

O segredo é redigido no `Debug` da configuração, então nunca aparece em log de
boot.

## Persistência

- **PostgreSQL** é a fonte da verdade. SeaORM mapeia tabelas em
  `database/entity/` (gerado por `just entities`).
- **Redis** guarda dados efêmeros: refresh tokens (com família), rate limit de
  login e respostas idempotentes. Redis fora do ar degrada a experiência
  (rate limit e idempotência viram fail-open), não corrompe dados. Cache de
  leitura fica para quando houver um caminho quente medido (ver ADR).
- **Migrations** ficam no crate (`src/migrations/`) e rodam por comando explícito
  (`just migrate`), nunca no boot da API. Isso evita corrida entre réplicas e
  mantém o boot rápido e previsível.

Convenções: UUID v7 como chave, `timestamptz` em UTC, `snake_case` no banco,
`camelCase` no JSON, `active` como soft delete e `version` para concorrência
otimista.

## Autenticação

Detalhes de ameaça e decisão estão em [`security.md`](security.md). Em resumo:

- **Access token** JWT HS256, 15 min, validado por assinatura/expiração no
  extractor, sem ida ao banco.
- **Refresh token** opaco, hasheado no Redis, rotacionado a cada uso
  (script Lua com marcador de uso), 7 dias, ligado a uma família.
- **`token_version`** no usuário: ao trocar senha/papel ou desativar, incrementa
  e invalida refresh tokens e sessões sensíveis imediatamente.
- Autorização por **permissão** (`auth.require(USERS_WRITE)`), não por `if role
  == "admin"`. Assim, adicionar um papel novo não exige caçar comparações.
- **Perfil próprio** (`PATCH /auth/me`, `PUT /auth/me/password`) usa
  `profile:write`; a troca de senha confere a senha atual e encerra todas as
  sessões via `token_version`.

## Rate limit, idempotência e métricas

Quatro camadas transversais, cada uma com um custo e uma decisão:

| Camada | Onde | Decisão |
|---|---|---|
| IP do cliente | `core/client_ip.rs` | só confia em `X-Forwarded-For` com `server.trust_proxy_headers = true` |
| Rate limit | `core/auth/rate_limit.rs` | janela fixa no Redis, por conta e IP; fail-open |
| Idempotência | `core/idempotency/` | obrigatória em `POST` de recurso; resposta 24 h; lock 60 s |
| Métricas | `core/metrics.rs` | recorder global via `OnceLock`; `/metrics` fora da medição |

O rate limit é aplicado no service de login, depois da validação do corpo e
antes de consultar senha: e-mail inexistente e senha errada contam igual, sem
enumeração. A chave usa SHA-256 do e-mail normalizado e do IP, então o Redis não
guarda PII.

A idempotência assina método, caminho, `sub` do access token e a chave. Usar o
`sub` (e não o token inteiro) mantém o replay válido após renovar o access
token, e ainda impede que a chave de um usuário reproduza a resposta de outro.
O replay devolve status, `content-type`, `location`, `etag` e corpo; repetir
durante a primeira execução devolve `409`; `5xx` não é gravado.

`/api/v1/auth/*` fica fora do middleware: a resposta carrega tokens de sessão e
um replay devolveria refresh já rotacionado, o que revogaria a família. A regra
é "`POST` de recurso sob `/api/v1` exige a chave; sessão não é recurso".

## Observabilidade

- Logs estruturados com `x-request-id`; 5xx genérico para o cliente.
- `/metrics` expõe `axum_http_requests_total`, `..._duration_seconds` e
  `..._pending` (axum-prometheus).
- `worker` loga resumo e duração de cada job.
- O endpoint de métricas não tem autenticação; restrinja na rede/proxy.

## Deploy

- `Dockerfile` multi-stage: build em `rust:1.99.1-slim`, runtime
  `debian:bookworm-slim`, UID 10001.
- `api` e `worker` usam a mesma imagem com comandos diferentes; `migrate` fica
  no perfil `tools` do compose.
- Migrations não rodam no boot: `docker compose --profile tools run --rm migrate`.
- CI (`quality` + `docker`) cobre fmt, clippy, nextest, `cargo deny` e build da
  imagem.

## E-mail e worker

O envio de e-mail é assíncrono por **transactional outbox**, não por fila:

```text
users::service::create
  BEGIN
    INSERT users
    INSERT email_outbox (pending, dedupe_key)
  COMMIT

worker (tokio-cron-scheduler)
  a cada 15 s: SELECT ... FOR UPDATE SKIP LOCKED → Mailer::send
  diário 03:00: remove e-mails `sent` com mais de 30 dias
```

Por que outbox no Postgres:

- o commit do negócio e a intenção de enviar são atômicos; não existe janela
  "usuário criado, e-mail perdido";
- `FOR UPDATE SKIP LOCKED` permite mais de um worker sem bloqueio mútuo;
- não exige mudar a persistência do Redis (hoje configurado como cache, com
  `allkeys-lru` e sem AOF), que poderia descartar mensagens;
- o volume de e-mail de um projeto novo cabe folgado em polling de 15 s.

O contrato é **at-least-once**: `dedupe_key` evita linha duplicada, mas o provedor
pode receber o mesmo e-mail duas vezes se o worker cair entre o envio e o
`UPDATE`. O envio é idempotente por natureza para convite/reset.

O worker só roda quando `jobs.enabled = true`; ele não sobe HTTP nem valida o
segredo JWT. `Mailer` (`core/mail/`) tem duas implementações:

| Provider | Uso | Comportamento |
|---|---|---|
| `log` | desenvolvimento | registra remetente, destinatário mascarado e assunto |
| `smtp` | produção | envia por SMTP/TLS via `lettre` |

O endereço completo nunca vai para o log no provider `log`.

No desenvolvimento, `just mail-up` sobe o Mailpit (SMTP `1025`, UI `8025`) e o
provider `smtp` aponta para `smtp://127.0.0.1:1025`. Assim o caminho real
(outbox → worker → SMTP) é exercitado sem enviar nada para fora. Mailpit escuta
só em `127.0.0.1` e não existe no deploy de produção.

## Por que não Keycloak

O alvo é um template de projeto novo, com um domínio de usuários simples
(`admin`/`user`) e deploy em 2 vCPU/2 GB. Um identity provider externo
adicionaria um serviço, um banco e um fluxo de configuração para resolver um
problema que o template não tem. Keycloak continua sendo a escolha certa quando
há SSO corporativo, federação ou múltiplos clientes — não é o caso aqui.
