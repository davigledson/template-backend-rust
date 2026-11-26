# Testes

## Piramide do projeto

```text
        ▲  poucos, lentos
   integração (testcontainers + axum-test)
  ─────────────────────────────────────────
   unitários de service, DTO, JWT, permissão
        ▼  muitos, rápidos
```

- **Unitário**: regra pura, sem banco e sem HTTP. Roda em milissegundos.
- **Integração**: rota real + Postgres/Redis reais em container.
- Não usamos mock de banco: mock de SQL testa o mock, não a query.

## Comandos

| Comando | Uso |
|---|---|
| `just test` | tudo (`cargo nextest run`) |
| `just test-unit` | só a lib |
| `just test-verbose` | com `println!`/`tracing` visíveis |
| `cargo test nome_do_teste` | um teste específico |
| `just ci-fast` | fmt + clippy + check + testes unitários |
| `just ci-full` | fmt + clippy + nextest + deny |
| `just ci` | pipeline completo local |
| `just hooks` | instala pre-commit/pre-push neste clone |

`nextest` roda cada teste em processo próprio e faz retry de teste instável
(`nextest.toml`: 1 retry local, 2 no perfil `ci`, timeout de 60 s).

O crate tem `#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]`.
`clippy.toml` libera `unwrap`/`expect` **em testes** — produção continua proibida.

## Testes unitários

Ficam no fim do arquivo testado:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usuario_comum_nao_gerencia_usuarios() {
        assert!(Role::User.can(PROFILE_WRITE));
        assert!(!Role::User.can(USERS_READ));
    }
}
```

O que sempre tem teste:

- política de senha (`hash`/`verify`, senha curta);
- claims do JWT (emitir/decodificar, token adulterado, outro segredo);
- permissões por papel;
- extração/validação de DTO (e-mail, paginação);
- regra de service sem I/O (normalização, cálculo, transição de estado).

Não escreva teste que só repete o corpo da função. Teste **comportamento
observável** e casos de borda: vazio, limite, duplicado, não encontrado, sem
permissão.

## Testes de integração

Dependências já no `Cargo.toml`: `axum-test`, `testcontainers` 0.27 e
`testcontainers-modules` 0.15 (`postgres`, `redis`). Os helpers ficam em
`tests/common/mod.rs` (`Harness::spawn`, login, mailers) e cada binário sobe seu
próprio Postgres 18 e Redis 8:

- `tests/api_flow.rs`: criação, listagem, autorização, troca de papel, soft
  delete, reset de senha, perfil, refresh, rate limit, idempotência, OpenAPI e
  métricas;
- `tests/email_flow.rs`: outbox, entrega, dedupe, retry com backoff,
  reprocessamento e corpo apagado após o envio;
- `tests/email_verification.rs`: reenvio público sem sessão, rate limit por
  conta, troca de e-mail invalidando o código anterior, confirmação manual,
  auditoria e login com `require_email_verified = true`;
- `tests/hardening.rs`: cabeçalhos de segurança, `no-store`, HSTS, `/metrics`
  com e sem token e documentação desligada;
- `tests/p2.rs`: expurgo da auditoria pela retenção, limpeza de metadados de
  sessão e invalidação do código de confirmação anterior.

O bloco de outbox prova as garantias:

- criar usuário grava `welcome` na mesma transação;
- `process_pending` envia e marca `sent`;
- `dedupe_key` repetida não cria segunda linha;
- falha do `Mailer` volta para `pending` com `attempts` incrementado e
  `last_error` preenchido.

`tests/api_flow.rs` cobre rate limit (5 falhas → `429`), reuso de refresh
(família revogada) e `/metrics` com `axum_http_requests_total`. A idempotência
tem asserções próprias: `POST` sem chave → `400`; mesma chave → mesmo id,
`Location` preservado e nada duplicado; login com a mesma chave → tokens
diferentes; replay após renovar o access token continua devolvendo a primeira
resposta.

Esqueleto de um teste de auth:

```rust
use axum_test::TestServer;
use testcontainers::runners::AsyncRunner;
use testcontainers_modules::{postgres::Postgres, redis::Redis};

#[tokio::test]
async fn login_e_me() {
    let pg = Postgres::default().start().await.expect("postgres");
    let redis = Redis::default().start().await.expect("redis");

    let pg_port = pg.get_host_port_ipv4(5432).await.expect("porta");
    // monte a URL com pg_port, rode as migrations via Migrator::up e construa
    // Settings/AppState de teste; depois:
    let server = TestServer::new(app::build(state)).expect("server");

    let response = server
        .post("/api/v1/auth/login")
        .json(&serde_json::json!({
            "email": "admin@teste.local",
            "password": "uma-senha-bem-longa-123"
        }))
        .await;

    response.assert_status_ok();
    assert!(response.json::<serde_json::Value>()["accessToken"].is_string());
}
```

Regras para integração:

- **container por binário de teste**, não por teste (subir Postgres custa
  segundos); use `Harness` para compartilhar dentro do mesmo fluxo;
- migrations rodam no setup (`Migrator::up`);
- cada teste usa dados próprios (e-mail único) e não assume ordem;
- teste sempre asserta status, `content-type` e o campo relevante;
- Redis pode ser substituído por um fake só quando o comportamento testado não
  for o Redis.

## O que um endpoint novo precisa provar

- [ ] caminho feliz (status + corpo)
- [ ] entrada inválida → 400 `problem+json`
- [ ] sem token → 401
- [ ] sem permissão → 403
- [ ] recurso inexistente → 404
- [ ] conflito/duplicidade → 409
- [ ] isolamento: usuário A não acessa recurso de B
- [ ] `POST` sem `Idempotency-Key` → 400 (recurso sob `/api/v1`)
- [ ] `POST` com `Idempotency-Key`: repetir devolve a mesma resposta (incluindo
      `Location`) e não duplica

## Teste de contrato do OpenAPI

`src/routes/openapi.rs` tem testes que montam o documento sem subir servidor e
verificam que as rotas e o esquema `bearer` existem. Ao adicionar um módulo, ele
passa a ser coberto automaticamente; se uma rota desaparecer do documento, o
teste falha.

## Smoke manual

Antes de dizer que terminou, com a API rodando em background:

```bash
curl -sf localhost:8080/health/ready

# auth
curl -s -X POST localhost:8080/api/v1/auth/login \
  -H 'content-type: application/json' \
  -d '{"email":"admin@teste.local","password":"..."}' | jq .accessToken
```

Fluxo completo de auth: login → `me` → refresh (token antigo deve dar 401) →
logout → refresh (401). Nunca deixe o servidor em foreground no chat; pare o
processo ao terminar.

Para validar e-mail de ponta a ponta no dev:

```bash
just mail-up
APP__MAIL__PROVIDER=smtp APP__MAIL__SMTP_URL=smtp://127.0.0.1:1025 just worker

# UI: http://localhost:8025
# API do Mailpit: http://localhost:8025/api/v1/messages
```

Crie um usuário pela API, rode o worker e confira destinatário e assunto no
Mailpit. O teste de integração continua com `RecordingMailer`; Mailpit é para
smoke manual e inspeção visual.

## CI

O CI é dividido em três momentos:

- **PR:** só o job `quality` (fmt, clippy, nextest e `cargo deny`). É o feedback
  rápido do desenvolvimento.
- **`main`, tag `v*` ou manual:** `quality` + `docker` + `compose`. O `docker`
  builda a imagem de produção e o `compose` sobe Postgres/Redis, aplica
  migrations, sobe API/worker, checa health/métricas e testa login inválido.
- **Pós-deploy:** o workflow `smoke.yml` roda contra uma URL real, com
  `workflow_dispatch` ou `workflow_call` no pipeline de deploy. Ele checa health,
  cabeçalhos, login inválido e `/metrics` quando `METRICS_TOKEN` está definido.

O loop local deve ser mais rápido que o CI:

```bash
just ci-fast   # fmt + clippy + check + testes unitários (sem containers)
just ci-full   # nextest completo + deny
just hooks     # instala pre-commit/pre-push no clone atual
```

Testcontainers usa o Docker do runner para Postgres e Redis. Falha de lint é
falha de build. Teste instável deve ser corrigido, não marcado como `#[ignore]`
sem justificativa e prazo.

O snapshot do OpenAPI é um teste normal (`routes::openapi`). Mudou o contrato de
propósito? Rode `INSTA_UPDATE=always cargo test --lib routes::openapi` e revise
o `.snap` no diff. Sem aceite, o teste falha e mostra o delta.
