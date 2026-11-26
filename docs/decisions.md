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
dias, consumido por script atômico com marcador de uso e reemitido a cada
refresh (famílias entram em ADR posterior).

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
por refresh token (fase 3), rate limit de login e idempotência (fase 7, ambos
implementados). Cache
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

---

## 2026-11-24 — Rate limit de login fail-open

**Contexto**: bloquear login quando o Redis cai protege contra força bruta, mas
transforma o cache em dependência crítica de disponibilidade.

**Decisão**: limitar por conta (5 falhas) e por IP (20) em janela fixa de 15 min,
com contadores hasheados no Redis. Se o Redis estiver indisponível, o login
segue e o erro vai para o log; o proxy deve aplicar um limite por IP como
segunda barreira.

**Consequência**: indisponibilidade de cache não derruba a autenticação. Durante
uma falha do Redis, a proteção fica reduzida ao proxy.

---

## 2026-11-24 — Famílias de refresh token

**Contexto**: a rotação simples detecta token já consumido, mas não derruba as
sessões derivadas dele. Um token roubado poderia ser usado antes de o legítimo
rotacionar.

**Decisão**: cada login cria uma família; a rotação mantém a família. O consumo
é atômico e grava um marcador "já usado" com TTL de 7 dias. Reapresentar um
token usado revoga a família inteira (`auth:refresh-family:<id>:revoked`), e
todos os refreshes daquela família passam a falhar.

**Consequência**: reuso derruba a sessão legítima junto, o que é o comportamento
desejado. Um novo login cria outra família e não é afetado.

---

## 2026-11-24 — Idempotência opcional por `Idempotency-Key`

**Contexto**: `POST` sem ID natural pode duplicar em retry de rede. Exigir a
chave em todo `POST` quebraria clientes simples e o próprio login.

**Decisão**: quando o cabeçalho existe, a primeira resposta é gravada por 24 h
(status, `content-type` e corpo) no Redis, com assinatura que inclui método,
caminho e hash do `Authorization`. Repetição durante a execução recebe `409`.
Sem o cabeçalho, o comportamento é o normal. Redis fora do ar é fail-open.

**Consequência**: retry seguro fica a cargo do cliente, mas o template oferece o
mecanismo pronto. A resposta gravada não pode conter segredo além do que o
próprio endpoint já devolve.

---

## 2026-11-24 — Fase 8: idempotência obrigatória, `sub` na assinatura e `/auth` fora

**Contexto**: a fase 7 deixou três arestas. (1) O middleware cobria qualquer
`POST`, inclusive `/auth/login` e `/auth/refresh`: com `Idempotency-Key`, a
resposta com tokens ficava 24 h no Redis e um replay após rotação devolvia um
refresh já usado, disparando a revogação da família. (2) O `AGENTS.md` dizia que
`POST` sem ID natural exige a chave, mas o código só a honrava se existisse.
(3) A assinatura usava o hash do `Authorization`, então renovar o access token
fazia o retry legítimo perder o replay.

**Decisão**: (1) só `POST` sob `/api/v1` de recurso exige a chave, e
`/api/v1/auth/*` fica de fora; (2) `Idempotency-Key` ausente devolve `400`;
(3) a assinatura usa o claim `sub` do token (com hash do cabeçalho como
fallback para token inválido); (4) o replay preserva apenas `content-type`,
`location` e `etag`; (5) `POST /users` devolve `Location`.

**Consequência**: o contrato fica estrito e previsível; clientes precisam enviar
o cabeçalho em criação. Sessões nunca são cacheadas no Redis de idempotência.

**Revisitar se**: aparecer `POST` público (ex.: contato, webhook de entrada) em
que exigir a chave seja atrito desnecessário — aí a exceção entra na mesma lista
de `/auth`.

---

## 2026-11-24 — Auditoria append-only das ações privilegiadas

**Contexto**: com mais de um admin, o banco registrava que uma linha mudou
(`updated_at`, `version`), mas não quem mudou. Log de aplicação é efêmero, não é
transacional e não é consultável; ações da CLI nem passam pelo HTTP.

**Decisão**: tabela `audit_log` append-only, dona do módulo `audit`, gravada na
mesma transação da mudança via `audit::service::record_user`. Ações
privilegiadas de `users` (criar, editar, mudar papel, mudar situação, redefinir
senha) e os comandos da CLI. `changes` guarda só o diff, sem senha/hash/token.
`GET /api/v1/audit` exige a permissão nova `audit:read`.

**Consequência**: falha ao gravar auditoria derruba a transação de negócio —
auditoria é requisito, não acessório. Sem FK para `users`, porque o registro é
um retrato histórico. Sem expurgo automático: retenção é decisão de operação.

**Revisitar se**: o volume exigir particionamento por mês, ou se a leitura
precisar de `require_session` (hoje só valida o JWT).

---

## 2026-11-24 — A auditoria não cobre ações do próprio usuário

**Contexto**: o usuário pode alterar o próprio nome e a própria senha. Seria
possível auditar tudo.

**Decisão**: só ações privilegiadas entram. `PATCH /auth/me` e
`PUT /auth/me/password` ficam fora; a troca de senha já dispara e-mail e sobe
`token_version`.

**Consequência**: a auditoria responde "quem administrou quem", não "o que cada
pessoa fez na própria conta". Se um dia houver exigência de rastrear login e
autoatendimento, isso entra como evento de segurança separado, com retenção
própria.

---

## 2026-11-25 — Recuperação de senha com código opaco e resposta uniforme

**Contexto**: faltava o caminho de quem esqueceu a senha. Um endpoint que revela
se o e-mail existe entrega enumeração de contas; um link mágico exige front com
rota dedicada e muda o contrato de erro.

**Decisão**: `POST /auth/password/forgot` recebe o e-mail e devolve **204
sempre**. A conta existente recebe um código opaco de 30 minutos (hash no
Redis, uso único, atômico), entregue pelo outbox. `POST /auth/password/reset`
recebe código + nova senha, incrementa `token_version` e derruba as sessões. O
código guarda o `token_version` e o fingerprint do e-mail da emissão: mudança de
senha, papel ou e-mail o invalida. Consumir o código também confirma o e-mail,
porque ele só chegou lá se a caixa pertence ao usuário. Rate limit em chaves separadas das do login.

**Consequência**: o front não consegue dizer "e-mail não encontrado" — e não
deve. O usuário que errou o próprio e-mail não recebe nada e precisa tentar de
novo. O Redis passa a ser dependência do fluxo: sem ele, recuperação não
funciona (aceito; o modelo já é efêmero).

**Revisitar se**: o produto exigir deep link com token na URL (aí o código vira
token de página) ou TTL diferente por risco.

---

## 2026-11-25 — Verificação de e-mail opcional, controlada por configuração

**Contexto**: o template já envia e-mails transacionais, mas exigir confirmação
no login quebra quem sobe a API sem SMTP.

**Decisão**: `email_verified_at` na tabela `users`; código de 7 dias emitido na
criação e na troca de e-mail. O login só exige confirmação com
`security.require_email_verified = true`. Os endpoints de confirmação e reenvio
existem sempre.

**Consequência**: em desenvolvimento (provider `log`) o fluxo funciona sem SMTP;
em produção, ligar a exigência é uma decisão de configuração, com um passo de
rollout. Trocar o e-mail zera a confirmação, porque o endereço novo ainda não
foi provado.

**Revisitar se**: surgir requisito de bloquear tudo até confirmar (então o
endpoint de login recusa antes de emitir token e o front precisa de tela de
"reenviar").

---

## 2026-11-26 — Confirmação amarrada ao endereço e reenvio público

**Contexto**: o código de confirmação guardava só o `user_id`. Trocar o e-mail
depois da emissão ainda validava o endereço novo, e o reenvio exigia login —
justamente o que o usuário não consegue fazer quando a exigência de e-mail
confirmado está ligada.

**Decisão**: o valor no Redis carrega `token_version` e fingerprint do e-mail,
como o código de recuperação; o consumo confere os dois no banco. O reenvio
passa a ser público por e-mail, com `204` sempre e rate limit por conta/IP. O
boot recusa `require_email_verified = true` com `mail.provider = log`. O corpo
do outbox é apagado ao marcar `sent`.

**Consequência**: trocar e-mail senha/papel invalida códigos pendentes; não há
enumeração no reenvio; a base existente precisa de `admin verify-email --all`
antes de ligar a flag. Códigos antigos no Redis expiram sozinhos em 7 dias.

**Revisitar se**: o reenvio público virar alvo de abuso mesmo com o rate limit
(aí entra captcha ou prova de trabalho).

---

## 2026-11-26 — Cabeçalhos, cache, docs e métricas configuráveis

**Contexto**: a API não enviava cabeçalhos de segurança, podia ter resposta
autenticada cacheada, expunha `/docs` e `/metrics` em qualquer ambiente e não
tinha como desligar a documentação.

**Decisão**: middleware único adiciona `nosniff`, `DENY`, `no-referrer`,
`Permissions-Policy` e HSTS opcional; `/api/v1` e `/metrics` levam
`Cache-Control: no-store`. `docs.enabled` controla `/docs` e
`/api-docs/openapi.json` (produção desliga). `metrics.enabled` controla a rota e
`metrics.token` adiciona Bearer opcional com comparação em tempo constante.

**Consequência**: o proxy ainda pode reforçar HSTS/CSP e esconder rotas. Sem
token, `/metrics` continua público por decisão explícita; a rede precisa
restringir. A UI de documentação deixa de existir por padrão em produção.

**Revisitar se**: for necessário expor métricas em porta separada (aí o listener
dedicado substitui o token) ou servir a documentação publicamente.

---

## 2026-11-26 — Retenção da auditoria, índice por ação e código único

**Contexto**: a auditoria crescia para sempre, o filtro por `action` não tinha
índice, o `admin reset-password` deixava metadados de sessão no Redis e um
reenvio de confirmação mantinha vários códigos válidos ao mesmo tempo.

**Decisão**: `[audit] retention_days` (padrão 365, `0` desliga) alimenta o job
diário do worker; migration adiciona `audit_log_action_created_idx`; o CLI de
reset limpa as sessões pelo mesmo service da API; e a emissão de confirmação
guarda um ponteiro `auth:email-verify:latest:<user>` que apaga o código anterior
e só aceita o mais recente.

**Consequência**: a retenção passa a ser uma decisão explícita de configuração;
auditoria antiga some por política, não por acidente. Códigos emitidos antes
desta mudança (sem ponteiro) deixam de valer após o deploy; o usuário pede
reenvio. O reset por CLI agora depende do Redis estar acessível, mas a falha é
best-effort: o `token_version` já invalidou os tokens.

**Revisitar se**: compliance exigir retenção maior/legal hold ou auditoria
imutável em storage externo.

---

## 2026-11-26 — CI rápido no PR e jobs pesados fora do caminho

**Contexto**: build Docker e smoke de compose rodavam em todo PR, deixando o
loop de desenvolvimento lento. A maior parte do trabalho acontece em localhost.

**Decisão**: `quality` (fmt, clippy, nextest, deny) roda em PR e push. `docker` e
`compose` rodam em `main`, tag `v*` e `workflow_dispatch`. O `smoke.yml` separado
testa uma URL publicada, via dispatch ou `workflow_call` do deploy. O loop local
ganhou `just ci-fast`, `just ci-full` e hooks opcionais.

**Consequência**: PR recebe feedback rápido; regressão de imagem/compose é
detectada antes do release, não a cada commit. O smoke pós-deploy depende de a
URL e o token de métricas existirem no ambiente.

**Revisitar se**: o número de merges na `main` tornar o build/compose caro, aí
entra filtro por caminho (`paths`) ou execução noturna.

---

## 2026-11-25 — Sessões por dispositivo no Redis, não no banco

**Contexto**: o refresh já é uma família por login. O usuário não tinha como ver
ou encerrar os outros dispositivos.

**Decisão**: metadados de sessão (`created_at`, `last_seen_at`, IP, User-Agent)
ficam em `auth:session:<user_id>:<family_id>` no Redis, com o TTL do refresh. O
`fid` entra no access token para marcar a sessão atual. Encerrar revoga a
família; a sessão atual só sai por logout.

**Consequência**: nada de tabela de sessões, nada de escrita no Postgres a cada
refresh — só Redis. Reiniciar o Redis perde a lista (e as sessões), como já
acontecia. O IP/User-Agent é o que o cliente declarou; serve para o usuário
reconhecer o dispositivo, não como prova forense.

**Revisitar se**: surgir exigência de auditoria de login persistente ou de
"confiar neste dispositivo" (aí o registro vai para o banco).
