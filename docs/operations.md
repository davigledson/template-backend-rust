# Operação

Runbook do que existe hoje: processos, backup, outbox, métricas e incidentes
comuns. Para publicar, ver [`../deploy.md`](../deploy.md). Para o tamanho do
host e quando crescer, ver `capacity.md`.

## Processos

| Binário | Papel | Observação |
|---|---|---|
| `api` | HTTP em `/api/v1`, `/docs` (se ligado) e `/metrics` (se ligado) | sem cron; pode ter mais de uma instância |
| `worker` | cron do outbox (15 s) e limpeza diária (outbox + auditoria) | uma instância é o padrão; várias são suportadas pelo `SKIP LOCKED` |
| `migrator` | aplica migrations e sai | nunca roda no boot da API |
| `admin` | bootstrap e manutenção | não é serviço |

O worker precisa estar rodando para e-mail sair do `email_outbox`. Sem ele, o
e-mail fica `pending` para sempre.

Os comandos com `docker/compose.deploy.yml` abaixo exigem as mesmas variáveis do
deploy no ambiente (`POSTGRES_PASSWORD`, `APP__DATABASE__URL` e
`APP__JWT__SECRET`) ou um `APP_ENV_FILE` com elas.

## Deploy

Todos os comandos abaixo usam `docker/compose.deploy.yml`. Com `just`, os
atalhos são `just prod-migrate`, `just prod-up`, `just prod-down` e
`just prod-logs`.

```bash
# 1. schema (explícito, antes de subir a API nova)
just prod-migrate

# 2. aplicação (infra + api + worker)
just prod-up
```

Equivalente sem `just`:

```bash
docker compose --project-directory . -f docker/compose.deploy.yml \
  --profile tools run --rm --build migrate
docker compose --project-directory . -f docker/compose.deploy.yml \
  --profile app up -d --build api worker
```

Verificação em três passos:

```bash
curl -fsS http://127.0.0.1:8080/health          # processo vivo
curl -fsS http://127.0.0.1:8080/health/ready    # banco e redis respondem
curl -fsS http://127.0.0.1:8080/metrics | head  # métricas expostas (sem token)
```

Se `APP__METRICS__TOKEN` estiver definido, use
`-H "authorization: Bearer <token>"`. O perfil de produção desliga `/docs` por
padrão; ligue `APP__DOCS__ENABLED=true` apenas atrás de proteção.

**Rollback**: volte a tag da imagem e suba de novo. Migration é
*forward-only*: se a versão nova exigiu coluna, o binário antigo só volta se
a migration for compatível (coluna nullable/default). Escreva migrations
assim.

Regra: nunca `docker compose --project-directory . -f docker/compose.deploy.yml down -v` em produção — o `-v` apaga o volume
`pgdata`.

## Smoke pós-deploy

O workflow `smoke.yml` testa uma URL publicada, não containers locais:

```bash
gh workflow run smoke.yml -f base_url=https://api.exemplo.com
```

Ele verifica `/health`, `/health/ready`, cabeçalhos de segurança, login inválido
(`401`) e, quando o secret `METRICS_TOKEN` existe, `/metrics`. O mesmo workflow
pode ser chamado por `workflow_call` no pipeline de deploy.

## Backup e restore

O Postgres é a única fonte de verdade. O Redis guarda refresh token, rate
limit e idempotência: perder o Redis derruba todas as sessões e libera os
bloqueios, mas não corrompe dados.

```bash
# backup
docker compose --project-directory . -f docker/compose.deploy.yml exec -T postgres pg_dump -U app -Fc app > app-$(date +%F).dump

# restore (banco vazio, com a API parada)
docker compose --project-directory . -f docker/compose.deploy.yml --profile app stop api worker
docker compose --project-directory . -f docker/compose.deploy.yml exec -T postgres dropdb -U app --if-exists app
docker compose --project-directory . -f docker/compose.deploy.yml exec -T postgres createdb -U app
docker compose --project-directory . -f docker/compose.deploy.yml exec -T postgres pg_restore -U app -d app --clean --if-exists < app-2026-11-24.dump
docker compose --project-directory . -f docker/compose.deploy.yml --profile app start api worker
```

Teste o restore de verdade antes de precisar dele. Um dump que nunca foi
restaurado não é backup.

## Segredo JWT

`APP__JWT__SECRET` só vem do ambiente, mínimo 32 caracteres. Trocar o segredo
invalida todos os access tokens na hora (assinatura deixa de bater) e todos os
refresh tokens também, porque a validação do refresh confere o banco. Não há
rotação com `kid`: a troca é big bang, com todo mundo fazendo login de novo.

## Outbox de e-mail

Estados: `pending` → `processing` → `sent` ou `pending` (retry) → `failed`
após 5 tentativas. Backoff: 1, 2, 4, 8, 16 min. `dedupe_key` evita linha
duplicada. Ao marcar `sent`, o corpo é apagado; a linha fica só com metadados e
é removida após 30 dias pelo job diário. `failed` mantém o corpo para permitir
`retry` manual.

```bash
just outbox-list                      # failed, 20 itens
just outbox-list status=pending
just outbox-retry <id>                # volta para pending agora
just outbox-retry-all                 # todos os failed
just outbox-retry-all force=true      # zera attempts
```

Quando usar: provedor ficou fora e o `Retry-After` dele estourou o backoff.
Depois de religar o SMTP, `retry-all` entrega na próxima passada do worker
(15 s). `failed` não é reenviado sozinho; é decisão explícita.

Conferir a profundidade sem CLI:

```sql
select status, count(*) from email_outbox group by status order by 2 desc;
```

## Confirmação de e-mail e rollout

O reenvio é público (`POST /auth/email/resend` com o e-mail no corpo) e sempre
devolve `204`; o limite é por conta e por IP. O boot **recusa** subir com
`security.require_email_verified = true` e `mail.provider = log`.

Para ligar a exigência em uma base que já tem usuários:

```bash
# 1. valide o SMTP real e o worker entregando
just outbox-list status=failed

# 2. confirme quem já está pendente (ou confirme um por um)
just admin-verify-email usuario@empresa.com
just admin-verify-email-all

# 3. só então ligue a flag no ambiente e reinicie a API
# APP__SECURITY__REQUIRE_EMAIL_VERIFIED=true
```

Sem o backfill, todo usuário antigo com `email_verified_at` nulo recebe `403`
no login. O `admin verify-email` grava `user.email_verified` na auditoria.

## Auditoria

```bash
# tudo o que aconteceu com um usuário
curl -s "http://127.0.0.1:8080/api/v1/audit?targetId=<uuid>" -H "authorization: Bearer <token>"

# o que um admin fez, em um período
curl -s "http://127.0.0.1:8080/api/v1/audit?actorId=<uuid>&from=2026-11-01T00:00:00Z&to=2026-12-01T00:00:00Z"

# só mudanças de papel
curl -s "http://127.0.0.1:8080/api/v1/audit?action=user.role_changed"
```

`actorId` ausente no item significa CLI ou job. O job diário expurga registros
mais antigos que `audit.retention_days` (padrão 365; `0` mantém tudo). Nunca
edite ou apague uma linha por SQL fora dessa política sem registrar o motivo.

## Métricas

`/metrics` é Prometheus. Com `metrics.token` vazio fica sem autenticação:
**restrinja no proxy** e não exponha na internet. Com token definido (mínimo 16
caracteres), exige `Authorization: Bearer <token>`; a comparação é em tempo
constante. Para desligar a rota, use `metrics.enabled = false`.

| Métrica | Tipo | Lê assim |
|---|---|---|
| `axum_http_requests_total` / `_duration_seconds` | counter/histogram | tráfego, erro por rota, p95 |
| `axum_http_requests_pending` | gauge | requisições em andamento |
| `app_email_outbox_pending` / `_failed` | gauge | fila de e-mail e falhas terminais (API lê o banco a cada 30 s) |
| `app_email_sent_24h` | gauge | enviados nas últimas 24 h; janela rolável, não use `rate()` |
| `app_login_failure_total` | counter | senha/e-mail errados |
| `app_login_blocked_total{scope}` | counter | 429 por conta ou por IP |
| `app_refresh_reuse_total` | counter | reuso de refresh (possível roubo ou bug de front) |
| `app_idempotency_replay_total` | counter | retry legítimo reaproveitado |
| `app_idempotency_conflict_total` | counter | duas execuções simultâneas com a mesma chave |
| `app_password_reset_requested_total` | counter | pedidos de recuperação que geraram e-mail |
| `app_password_reset_blocked_total{scope}` | counter | pedidos recusados por rate limit (conta ou IP) |
| `app_password_reset_completed_total` | counter | senhas efetivamente redefinidas por código |
| `app_email_verified_total` | counter | e-mails que passaram de não confirmado para confirmado |
| `app_session_revoked_total` | counter | sessões encerradas (logout ou tela de dispositivos) |

Alertas que fazem sentido neste tamanho: `app_email_outbox_failed > 0` por
10 min, `app_refresh_reuse_total` crescendo, `up == 0` no scrape, p95 de
`/api/v1/auth/login` acima de 1 s e `app_password_reset_requested_total`
crescendo sem `app_password_reset_completed_total` (possível abuso ou e-mail
não entregue).

Os gauges de outbox são recalculados pelo processo da **API** a partir do
banco: o worker não expõe `/metrics`. Se o worker parar, `pending` sobe e
`sent_24h` para de crescer — é o sintoma esperado.

## Rate limit e lockout

- Chaves: `auth:login:fail:account:<sha256(email)>` e
  `auth:login:fail:ip:<sha256(ip)>`, janela de 15 min configurável.
- Recuperação de senha: `auth:pwreset:fail:account:*` (3) e
  `auth:pwreset:fail:ip:*` (10). Reenvio de confirmação:
  `auth:verify:fail:account:*` (3) e `auth:verify:fail:ip:*` (10).
- Login válido zera a conta, não o IP. Pedido de recuperação conta na hora,
  independentemente de a conta existir.
- Para liberar um usuário legítimo antes da janela:

```bash
# descubra pelo log (`scope=account`) e remova só a chave da conta
docker compose --project-directory . -f docker/compose.deploy.yml exec redis redis-cli --scan --pattern 'auth:login:fail:account:*'
docker compose --project-directory . -f docker/compose.deploy.yml exec redis redis-cli DEL 'auth:login:fail:account:<hash>'
```

Cuidado: o hash não revela o e-mail. Prefira esperar a janela ou identificar
o usuário pelo log do 429.

## Logs e rastreio

- JSON em produção (`log.format = json`), com `x-request-id` em cada linha.
- Para seguir uma requisição: pegue o `x-request-id` da resposta e filtre o
  log. O span do service herda o id do HTTP.
- 5xx devolve mensagem genérica; o detalhe (`DbErr`, caminho) fica só no log.
- Nada de senha, token ou e-mail completo no log; o provider `log` mascara o
  destinatário.

## Incidentes comuns

| Sintoma | Causa provável | Ação |
|---|---|---|
| `/health/ready` 503 | Postgres ou Redis fora | `just doctor`, logs do serviço; a API volta sozinha |
| Login 429 em massa | ataque ou cliente em loop | confirmar em `app_login_blocked_total`; bloquear IP no proxy |
| E-mail não chega | worker parado ou SMTP fora | `just outbox-list status=pending`, logs do worker |
| Recuperação devolve 204 e nada chega | conta inexistente/inativa, worker parado ou SMTP fora | conferir `app_password_reset_requested_total` e o outbox |
| Reset devolve 400 | código expirado, já usado ou senha mudou depois do pedido | pedir outro código (`forgot`) |
| Login 403 "confirme seu e-mail" | `security.require_email_verified = true` | `just admin-verify-email <email>` ou desligar a exigência |
| Outbox `failed` crescendo | provedor rejeitando | ler `last_error`, corrigir e `retry-all` |
| 409 na criação de usuário | `Idempotency-Key` reutilizada ou e-mail duplicado | conferir o `detail` do Problem |
| Sessões caíram todas | Redis reiniciado | esperado: refresh é opaco e vive no Redis |
| Latência alta no login | Argon2 + host saturado | ver CPU e `axum_http_requests_duration_seconds` |
