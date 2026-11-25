# Operação

Runbook do que existe hoje: processos, deploy, backup, outbox, métricas e
incidentes comuns. Para o tamanho do host e quando crescer, ver
`capacity.md`.

## Processos

| Binário | Papel | Observação |
|---|---|---|
| `api` | HTTP em `/api/v1`, `/docs` e `/metrics` | sem cron; pode ter mais de uma instância |
| `worker` | cron do outbox (15 s) e limpeza diária | **uma instância só** evita disputa de lease |
| `migrator` | aplica migrations e sai | nunca roda no boot da API |
| `admin` | bootstrap e manutenção | não é serviço |

O worker precisa estar rodando para e-mail sair do `email_outbox`. Sem ele, o
e-mail fica `pending` para sempre.

## Deploy

```bash
# 1. imagem
docker compose build

# 2. schema (explícito, antes de subir a API nova)
docker compose --profile tools run --rm migrate

# 3. aplicação
docker compose --profile app up -d
```

Verificação em três passos:

```bash
curl -fsS http://127.0.0.1:8080/health          # processo vivo
curl -fsS http://127.0.0.1:8080/health/ready    # banco e redis respondem
curl -fsS http://127.0.0.1:8080/metrics | head  # métricas expostas
```

**Rollback**: volte a tag da imagem e suba de novo. Migration é
*forward-only*: se a versão nova exigiu coluna, o binário antigo só volta se
a migration for compatível (coluna nullable/default). Escreva migrations
assim.

Regra: nunca `docker compose down -v` em produção — o `-v` apaga o volume
`pgdata`.

## Backup e restore

O Postgres é a única fonte de verdade. O Redis guarda refresh token, rate
limit e idempotência: perder o Redis derruba todas as sessões e libera os
bloqueios, mas não corrompe dados.

```bash
# backup
docker compose exec -T postgres pg_dump -U app -Fc app > app-$(date +%F).dump

# restore (banco vazio, com a API parada)
docker compose --profile app stop api worker
docker compose exec -T postgres dropdb -U app --if-exists app
docker compose exec -T postgres createdb -U app
docker compose exec -T postgres pg_restore -U app -d app --clean --if-exists < app-2026-11-24.dump
docker compose --profile app start api worker
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
duplicada. `sent` com mais de 30 dias é apagado pelo job diário.

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

## Auditoria

```bash
# tudo o que aconteceu com um usuário
curl -s "http://127.0.0.1:8080/api/v1/audit?targetId=<uuid>" -H "authorization: Bearer <token>"

# o que um admin fez, em um período
curl -s "http://127.0.0.1:8080/api/v1/audit?actorId=<uuid>&from=2026-11-01T00:00:00Z&to=2026-12-01T00:00:00Z"

# só mudanças de papel
curl -s "http://127.0.0.1:8080/api/v1/audit?action=user.role_changed"
```

`actorId` ausente no item significa CLI ou job. A tabela não tem expurgo
automático; defina retenção antes de produção e nunca edite uma linha por SQL
sem registrar o motivo.

## Métricas

`/metrics` é Prometheus, sem autenticação. **Restrinja no proxy**; não exponha
na internet.

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
  `auth:verify:fail:user:<uuid>` (3).
- Login válido zera a conta, não o IP. Pedido de recuperação conta na hora,
  independentemente de a conta existir.
- Para liberar um usuário legítimo antes da janela:

```bash
# descubra pelo log (`scope=account`) e remova só a chave da conta
docker compose exec redis redis-cli --scan --pattern 'auth:login:fail:account:*'
docker compose exec redis redis-cli DEL 'auth:login:fail:account:<hash>'
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
| `/health/ready` 503 | Postgres ou Redis fora | `docker compose ps`, logs do serviço; a API volta sozinha |
| Login 429 em massa | ataque ou cliente em loop | confirmar em `app_login_blocked_total`; bloquear IP no proxy |
| E-mail não chega | worker parado ou SMTP fora | `just outbox-list status=pending`, logs do worker |
| Recuperação devolve 204 e nada chega | conta inexistente/inativa, worker parado ou SMTP fora | conferir `app_password_reset_requested_total` e o outbox |
| Reset devolve 400 | código expirado, já usado ou senha mudou depois do pedido | pedir outro código (`forgot`) |
| Login 403 "confirme seu e-mail" | `security.require_email_verified = true` | confirmar pelo e-mail ou desligar a exigência |
| Outbox `failed` crescendo | provedor rejeitando | ler `last_error`, corrigir e `retry-all` |
| 409 na criação de usuário | `Idempotency-Key` reutilizada ou e-mail duplicado | conferir o `detail` do Problem |
| Sessões caíram todas | Redis reiniciado | esperado: refresh é opaco e vive no Redis |
| Latência alta no login | Argon2 + host saturado | ver CPU e `axum_http_requests_duration_seconds` |
