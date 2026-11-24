# Decisões (ADRs curtos)

Formato: **data — decisão**. Contexto, consequência e quando revisitar.

---

## 2026-11-23 — Rust com Axum + SeaORM + PostgreSQL + Redis

**Contexto**: precisamos de um backend para projetos novos, com baixo consumo de
memória (2 vCPU/2 GB), concorrência boa e tipagem forte.

**Decisão**: Axum 0.8 (HTTP), SeaORM 2.0 (ORM + migrations), PostgreSQL 18,
Redis 8.

**Consequência**: menos mágica que um framework full-stack, mas controle total do
runtime e do SQL. SeaORM 2.0 usa `async fn` nativo em traits.

**Revisitar se**: o time não tiver Rust; ou o domínio exigir ORM com identidade
própria (aí vale avaliar).

---

## 2026-11-23 — Monólito modular por feature

**Contexto**: camadas globais espalham uma feature por vários diretórios e
dificultam mudanças coesas.

**Decisão**: `modules/<feature>` contém controller, service, repository e dto. O
`core` só tem infraestrutura.

**Consequência**: feature muda em um lugar só; limites explícitos; extrair
serviço depois é mais fácil. Exige disciplina para não criar dependência entre
módulos.

**Revisitar se**: o domínio precisar de deploy independente — aí workspace ou
serviço separado.

---

## 2026-11-23 — Uma API + worker separado

**Contexto**: cron no mesmo processo da API compete por CPU e pode atrasar
requests.

**Decisão**: binário `api` (HTTP) e binário `worker` (cron), compartilhando
`core` e `modules`.

**Consequência**: deploy e escala independentes; logs separados. O worker não
expõe HTTP.

---

## 2026-11-23 — Cron com `tokio-cron-scheduler`

**Contexto**: jobs simples de agenda (limpeza, sincronização) não precisam de
fila com retry.

**Decisão**: `tokio-cron-scheduler` no worker.

**Consequência**: simples e suficiente. Job longo ou com retry/garantia vira
fila (`apalis` + Redis) — não force o scheduler.

---

## 2026-11-23 — Defaults para 2 vCPU / 2 GB

**Decisão**: pool de banco 8, pool Redis 8, `worker_threads = 2`, timeouts
curtos, body limit moderado.

**Consequência**: menos paralelismo, mais previsibilidade. Ajuste por ambiente
via `APP__*`, não no código.

---

## 2026-11-23 — SOLID pragmático e idempotência como critério

**Decisão**: interfaces pequenas, uma responsabilidade por camada, dependência de
abstrações quando há mais de uma implementação. Escritas relevantes aceitam
`Idempotency-Key`.

**Consequência**: menos abstração cerimonial; retry de cliente não duplica dado.

**Revisitar se**: a abstração não tiver segunda implementação — remova.

---

## 2026-11-23 — JWT próprio; Keycloak fora

**Contexto**: o domínio de identidade é `admin`/`user`, um único cliente e sem
SSO corporativo. Keycloak pertence a outro contexto (Rails/Prefeitura).

**Decisão**: access JWT HS256 emitido pela API; refresh opaco no Redis.

**Consequência**: um serviço menos para operar. Não há federação, MFA ou SSO
prontos.

**Revisitar se**: surgir SSO, múltiplos clientes ou requisito de MFA/federação.

---

## 2026-11-24 — Escopo do template: só `users` + auth

**Contexto**: `products`/CRUD genérico viravam exemplo morto, copiado sem
critério.

**Decisão**: o template entrega um domínio real e completo (usuários +
autenticação). Sem módulo de exemplo descartável.

**Consequência**: menos código para apagar em cada projeto novo; `users` é a
referência de padrão.

---

## 2026-11-24 — RBAC leve: papel no banco, permissões no código

**Contexto**: tabelas de papel/permissão são flexíveis, mas exigem CRUD, cache e
testes para um requisito que não existe.

**Decisão**: `users.role` (`admin`/`user`) no banco; catálogo de permissões
versionado em `core/auth/permission.rs`; handlers checam **permissão**.

**Consequência**: mudar política = mudar código (revisável, testável). Adicionar
papel não exige caçar `if role ==`.

**Revisitar se**: o cliente precisar criar papéis sem deploy — aí RBAC em
tabelas.

---

## 2026-11-24 — Cadastro só por admin; primeiro admin por CLI

**Contexto**: registro público abre spam e enumeração de contas. Seed com senha
no repositório é vazamento esperando acontecer.

**Decisão**: usuários são criados por admin autenticado; o primeiro admin vem do
binário `admin` (`create`/`promote`/`reset-password`), com senha por prompt ou
`ADMIN_PASSWORD`.

**Consequência**: bootstrap explícito e auditável; nenhuma rota pública de
registro.

---

## 2026-11-24 — Refresh opaco com rotação no Redis

**Decisão**: refresh de 64 caracteres hex, guardado como SHA-256 com TTL de 7
dias, consumido por `GETDEL` e reemitido a cada refresh.

**Consequência**: token vazado tem janela curta; reuso do antigo falha. Detecção
de reuso com revogação de família fica para o hardening.

**Revisitar se**: precisar de sessões simultâneas por dispositivo com gestão
individual — aí uma tabela de sessões.

---

## 2026-11-24 — `token_version` para revogação

**Decisão**: trocar senha/papel ou desativar incrementa `token_version`. Refresh
confere o valor; rotas sensíveis usam `require_session`.

**Consequência**: revogação imediata sem blacklist de JWT. O access token
continua válido por até 15 min em rotas não sensíveis — trade-off aceito.

---

## 2026-11-24 — Configuração em camadas

**Decisão**: `default.toml` → `config/<perfil>.toml` → `APP__*` → `.env` (só
`development`). Segredo JWT apenas por ambiente, mínimo 32 caracteres, validado
no boot.

**Consequência**: mesmo binário em todos os ambientes; produção não depende de
arquivo `.env`; erro de config falha cedo.

---

## 2026-11-24 — Migrations in-crate, por comando

**Decisão**: migrations em `src/migrations/`, aplicadas por `just migrate`
(binário `migrator`), nunca no boot da API.

**Consequência**: sem corrida entre réplicas e boot rápido. Forward-only: nunca
edite migration aplicada.

---

## 2026-11-24 — Contrato de API

**Decisão**: `/api/v1`, JSON `camelCase`, listas `{ items, page, perPage, total }`,
erros `application/problem+json` (RFC 9457), IDs UUID v7, datas `timestamptz`
UTC.

**Consequência**: frontends consomem um contrato previsível; erro tem o mesmo
formato em 400/401/403/404/409/422/500.

---

## 2026-11-24 — Soft delete e concorrência otimista

**Decisão**: desativar = `active = false` (sem `deleted_at`); `version` incrementa
em update e updates condicionais usam `WHERE version = ?`.

**Consequência**: histórico preservado e e-mail continua único. Exclusão física
só com necessidade explícita.

---

## 2026-11-24 — Validação com extractors próprios

**Contexto**: `axum-valid` valida antes do handler, mas a rejeição padrão não é
Problem Details.

**Decisão**: `core::extract::{ValidatedJson, ValidatedQuery}` validam com
`validator` e devolvem `AppError::Validation` (422 `application/problem+json`).

**Consequência**: contrato de erro uniforme; validação declarativa no DTO;
`axum-valid` saiu das dependências.

---

## 2026-11-24 — OpenAPI gerado do mesmo router

**Decisão**: handlers usam `#[utoipa::path]` e são registrados por `routes!` em
`OpenApiRouter`; `routes/openapi.rs` adiciona `/api/v1` e serve
`/api-docs/openapi.json` + Scalar em `/docs`.

**Consequência**: rota e documentação não divergem; testes de contrato falham se
uma rota sumir. O custo é aprender os atributos do `utoipa` ao criar endpoints.

---

## 2026-11-24 — `AGENTS.md` como contrato de execução

**Contexto**: decisões no histórico do chat se perdem; documentação longa não é
lida por agente.

**Decisão**: `AGENTS.md` curto, imperativo, com regras duras, comandos e
checklists. O "porquê" vai para `docs/`.

**Consequência**: agente e pessoa seguem o mesmo padrão; documentação profunda
continua existindo, mas não bloqueia o dia a dia.

---

## 2026-11-24 — Cache de leitura sob demanda (fase 5 cancelada)

**Contexto**: o plano previa cache Redis no `users`. Não existe caminho quente
medido; o CRUD admin é de baixo volume e o Postgres responde em poucos ms.

**Decisão**: não implementar cache de leitura agora. O Redis fica justificado
por refresh token (fase 3), rate limit de login e idempotência (fase 7). Cache
entra por módulo, apenas com medição que mostre o banco no caminho crítico.

**Consequência**: menos invalidação e menos estado para errar. Se o cache voltar,
a chave leva prefixo de ambiente e a invalidação acontece **depois** do commit;
Redis fora do ar deve degradar para o banco, nunca derrubar a leitura.

---

## 2026-11-24 — E-mail transacional por outbox no Postgres

**Contexto**: criar usuário e enviar e-mail são operações que não compartilham
transação. Publicar em fila depois do commit cria a janela "commitou e não
enfileirou"; enviar antes pode mandar e-mail de usuário que não existe.

**Decisão**: gravar a intenção em `email_outbox` na **mesma transação** da
mudança de negócio. O worker lê com `FOR UPDATE SKIP LOCKED`, envia atrás da
trait `Mailer` (`log` ou `smtp`) e marca `sent`, `pending` (retry com backoff) ou
`failed`. `dedupe_key` única evita linha duplicada; lease de 5 min recupera item
preso em `processing`.

**Alternativas descartadas**: fila no Redis (exigiria AOF e política de memória
adequada, hoje `allkeys-lru` sem persistência); envio no request (acopla
latência e perda ao caminho HTTP).

**Consequência**: entrega at-least-once, sem perda em restart; exige limpeza de
`sent` antigos (job diário) e tolerância a duplicata no provedor. Para métrica e
log, outbox é exagero: use `tokio::spawn` ou um job direto.

---

## 2026-11-24 — Troca da própria senha encerra todas as sessões

**Contexto**: `PUT /auth/me/password` poderia devolver um par novo e manter o
usuário logado, ou exigir login novamente.

**Decisão**: verificar a senha atual, incrementar `token_version` e devolver
`204`. Todas as sessões caem, inclusive a que fez a troca; nenhum token novo é
emitido.

**Consequência**: quem trocou a senha faz um login extra, mas uma sessão
possivelmente comprometida não sobrevive à troca. O aviso de alteração entra no
outbox na mesma transação.
