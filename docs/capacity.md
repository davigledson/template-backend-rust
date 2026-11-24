# Capacidade

Alvo do template: **2 vCPU / 2 GB** em um host único, com Postgres e Redis no
mesmo lugar. Tudo abaixo é ponto de partida; ajuste com medição.

## Orçamento de memória

| Processo | Limite no compose | Por quê |
|---|---:|---|
| Postgres | 768 MB | `shared_buffers=256MB` + conexões + cache |
| Redis | 160 MB | `maxmemory 128mb` + overhead |
| API | 384 MB | Tokio 2 threads, pool 8, payload limitado a 1 MB |
| Worker | 256 MB | cron curto, sem tráfego HTTP |
| Migrator | 384 MB | pico de migration; roda e sai |
| Mailpit (perfil `mail`) | 128 MB | só desenvolvimento |

Sobra folga para o kernel e para picos, o que evita OOM matar a API quando o
Postgres cresce. O `mem_limit` é teto, não reserva.

## Alavancas de configuração

| Ajuste | Onde | Padrão |
|---|---|---|
| Pool do banco | `[database] max_connections` | 8 |
| Conexões mínimas | `[database] min_connections` | 1 |
| Threads do Tokio | `[server] worker_threads` | 2 em produção, 0 = automático |
| Timeout de request | `[server] request_timeout_secs` | 30 |
| Limite de body | `[server] body_limit_mb` | 2 em dev, 1 em produção |
| Pool do Redis | `[redis] pool_size` | 8 |
| Outbox por lote | `BATCH_SIZE` em `email/service.rs` | 20 |
| Cron do outbox | `jobs/mod.rs` | a cada 15 s |

Regra prática: `max_connections` da API **vezes** o número de réplicas precisa
caber em `postgres max_connections` (40), com folga para `psql` e migrations.

## O que medir antes de crescer

1. **CPU**: `docker stats` ou `top`. Se o processo satura 2 vCPU em horário de
   pico, o gargalo é CPU (Argon2 no login é o suspeito mais comum).
2. **p95 HTTP**: `axum_http_requests_duration_seconds` por rota. Compare
   `/auth/login` (hash de senha) com `/users` (SQL).
3. **Fila de e-mail**: `app_email_outbox_pending` por mais de um ciclo e o
   `duration_ms` logado pelo worker a cada lote.
4. **Pool**: `select count(*), state from pg_stat_activity group by state;`.
   Muitas conexões `idle` com pool 8 indica que 8 é suficiente; fila de espera
   indica `max_connections` baixo ou query lenta.
5. **Memória**: `docker stats` por serviço contra a tabela acima.

## Quando crescer

| Sinal | Primeiro passo | Depois |
|---|---|---|
| CPU saturada com p95 alto | mais vCPU; manter `worker_threads` = vCPU | separar o worker em outro host |
| Postgres com I/O alto | índice que falta (ver `EXPLAIN`), shared_buffers | disco melhor |
| `pending` de e-mail crescendo | rodar worker com mais frequência | mais de um worker com lease (já suportado por `SKIP LOCKED`) |
| Redis perto de `maxmemory` | aumentar `maxmemory` | revisar TTLs; o cache de leitura nem existe |

Escala vertical resolve até 4 vCPU / 8 GB com este desenho. Acima disso,
separe Postgres do host da aplicação antes de pensar em microserviço.

## Referência do ACEC (Java) no mesmo hardware

Estimativa do plano original, para calibrar expectativa:

| Métrica | Java | Template Rust |
|---|---:|---:|
| Startup | 8–15 s | 0,1–0,3 s |
| RSS sob carga | 600–900 MB | 80–150 MB |
| RPS `GET /{id}` | 800–2.000 | 2.000–4.000 |
| p99 | 20–60 ms | 6–15 ms |

Números de referência, não garantia: medir no host real antes de decidir
capacidade de produção.
