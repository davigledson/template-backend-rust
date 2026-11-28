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
| Front (perfil `front`) | 64 MB | nginx servindo assets estáticos |
| Migrator | 384 MB | pico de migration; roda e sai |
| Mailpit (perfil `mail`) | 128 MB | só desenvolvimento |

O build da imagem do front **não** roda no servidor de produção: Node + Vite
consomem cerca de 1 GB e podem acionar o OOM killer. A imagem vem do CI.

Sobra folga para o kernel e para picos, o que evita OOM matar a API quando o
Postgres cresce. O `mem_limit` é teto, não reserva.

## Alavancas de configuração

| Ajuste | Onde | Padrão |
|---|---|---|
| Pool do banco | `[database] max_connections` | 8 |
| Conexões mínimas | `[database] min_connections` | 1 |
| Threads do Tokio | `[server] worker_threads` | 2 em produção, 0 = automático |
| Timeout de request | `[server] request_timeout_secs` | 30 |
| Statement timeout (API) | `[database] statement_timeout_secs` | 5; worker/migrations não usam |
| Limite de body | `[server] body_limit_mb` | 2 em dev, 1 em produção |
| Pool do Redis | `[redis] pool_size` | 8 |
| Tarefas de senha simultâneas | `MAX_CONCURRENT_PASSWORD_TASKS` em `core/auth/password.rs` | 2 (≈38 MiB de pico) |
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

## Benchmark

Antes de decidir capacidade, meça com build release e a mesma massa de dados:

```bash
cargo run --release --bin api
just admin-create bench@example.com "Bench" true
BENCH_EMAIL=bench@example.com BENCH_PASSWORD='...' just bench 10s 20
just db-stats
```

Registre uma linha por rodada (a tabela começa vazia de propósito; número sem
data e sem massa não serve para comparar):

| Data | Commit | Host | Dados | Endpoint | RPS | p50 | p95 | p99 |
|---|---|---|---|---|---:|---:|---:|---:|
| — | — | — | — | — | — | — | — | — |

Leitura do resultado:

- **Throughput não é latência.** Em rota presa ao banco, dobrar o RPS pode
  reduzir o p95 só 20-30%, porque o tempo é espera no Postgres.
- Comparação justa é antes/depois no mesmo host, mesmo dia e mesma massa. Debug,
  máquina quente e runner compartilhado distorcem.
- `statement_timeout` evita request pendurado; não acelera consulta. Query lenta
  se corrige com índice/consulta, não com limite maior.
- O login é medido à parte: Argon2 e rate limit mudam o resultado e cada
  iteração cria sessão.

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

No repositório da ACEC, o startup medido em desenvolvimento ficou entre 1,0 e
3,3 s (Spring Boot 4 com lazy initialization); em container, o palpite é 3-10 s.
O container roda com `-Xmx512m`, metaspace de 192 MB e `mem_limit: 900m`. Não
existe benchmark de throughput no repositório e o banco local tem só dados de
demonstração, então não há RPS confiável para comparar.

| Métrica | ACEC Java | Expectativa para este template |
|---|---|---|
| Startup | 1-3 s local; 3-10 s no container | 0,1-0,3 s |
| RSS em repouso | ~700 MB (limite 900 MB) | 80-150 MB |
| CRUD | baseline | 1,2-2,5x, conforme o endpoint |
| CPU (PDF/relatório) | baseline | 2-4x, se a biblioteca Rust for equivalente |
| SMTP/HTTP externo | baseline | ~1x: rede domina |

O ganho mais concreto no host de 1,9 GB é memória: liberar centenas de MB para
o Postgres costuma render mais que a linguagem. Esses fatores são ordem de
grandeza, não promessa; `just bench` no mesmo host e com massa real decide.
