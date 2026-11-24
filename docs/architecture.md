# Arquitetura

Este documento explica o **porquê** do desenho. As regras operacionais estão no
[`AGENTS.md`](../AGENTS.md).

## Visão geral

```text
cliente HTTP ──► api (Axum) ──► modules/<feature> ──► core (infra)
                                    │
                                    ├──► PostgreSQL (SeaORM)
                                    └──► Redis (cache + refresh tokens)

cron ─────────► worker (mesmo código, sem HTTP)
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

O `AppState` carrega `db`, `cache`, `jwt` e `config`; os serviços recebem só o
que precisam (`&state.db`, `&state.cache`), o que os deixa testáveis sem subir a
aplicação inteira.

## Ciclo de uma requisição

```text
TCP
 └─ camadas (core/http.rs): request-id → trace → cors → compress → timeout → body limit
     └─ router /api/v1
         └─ extractor (AuthUser, Json, Path, Query, Valid)
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
- **Redis** guarda dados efêmeros: refresh tokens e (na fase 5) cache de leitura.
  Redis fora do ar degrada a experiência, não corrompe dados.
- Migrations ficam no crate (`src/migrations/`) e rodam por comando explícito
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
  (`GETDEL`), 7 dias.
- **`token_version`** no usuário: ao trocar senha/papel ou desativar, incrementa
  e invalida refresh tokens e sessões sensíveis imediatamente.
- Autorização por **permissão** (`auth.require(USERS_WRITE)`), não por `if role
  == "admin"`. Assim, adicionar um papel novo não exige caçar comparações.

## Por que não Keycloak

O alvo é um template de projeto novo, com um domínio de usuários simples
(`admin`/`user`) e deploy em 2 vCPU/2 GB. Um identity provider externo
adicionaria um serviço, um banco e um fluxo de configuração para resolver um
problema que o template não tem. Keycloak continua sendo a escolha certa quando
há SSO corporativo, federação ou múltiplos clientes — não é o caso aqui.
