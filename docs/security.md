# Segurança

Decisões, garantias atuais e o que falta endurecer. Regras de uso diário estão no
[`AGENTS.md`](../AGENTS.md).

## Modelo de confiança

- O cliente é **não confiável**: todo dado de entrada é validado no DTO.
- O banco é a fonte da verdade de identidade e papel.
- O Redis é efêmero: perder o Redis derruba sessões de refresh, mas não corrompe
  dados.
- O segredo JWT é o ativo mais sensível do processo. Vaza = emitir token de
  qualquer usuário.

## Senhas

- **Argon2id** (`Argon2::default()`, parâmetros padrão do crate `argon2` 0.6).
- Salt aleatório por senha; o hash PHC é armazenado em `users.password_hash`.
- Mínimo de **12 caracteres**. O template não impõe composição (maiúscula,
  símbolo), porque comprimento importa mais que "sopa de regras".
- O mesmo `password::hash`/`verify` é usado pelo login e pelo CLI `admin` — não
  existe caminho alternativo de hash.
- Login com e-mail inexistente, inativo ou senha errada devolve **a mesma**
  mensagem (`credenciais inválidas`), evitando enumeração de contas.

> Melhoria para o projeto: checar senhas vazadas (`zxcvbn`/lista de comuns) e
> limitar tentativas por conta.

## Access token

| Item | Valor |
|---|---|
| Algoritmo | HS256 |
| TTL | 15 min (`jwt.access_ttl_min`) |
| Claims | `sub`, `email`, `role`, `token_version`, `fid` (família), `jti`, `iat`, `exp`, `iss`, `aud` |
| Validação | assinatura, `exp`, `iss`, `aud`, leeway de 5 s |
| Onde é validado | extractor `AuthUser`, sem consultar o banco |

O token **não** carrega a lista de permissões. O extractor deriva as permissões
do `role` no servidor, então mudar o mapa de permissões não exige reemitir token.
O custo é que um token recém-emitido reflete o papel no momento da emissão; por
isso rotas sensíveis chamam `require_session`.

`AuthUser` garante apenas "token legítimo e não expirado". Ele **não** sabe se o
usuário foi desativado, se a senha mudou ou se o papel mudou.

## Rotas sensíveis: `require_session`

```rust
let user = users::service::require_session(&state.db, &auth).await?;
```

Confere no banco:

1. o usuário existe;
2. `active = true`;
3. `token_version` do banco == do token;
4. `role` do banco == do token.

Qualquer divergência → `401` com "sessão expirada; faça login novamente".

Use em: `/auth/me`, gestão de usuários, troca de senha e qualquer operação
destrutiva. Não use em toda rota: é uma query a mais.

## Refresh token

- **Opaco**: 64 caracteres hex (dois UUID v4 sem hífen), sem significado.
- **Hasheado** com SHA-256 no Redis; o valor em claro só existe no cliente.
- Valor armazenado: `"{user_id}:{token_version}:{family_id}"`.
- TTL de 7 dias.
- **Rotação**: cada refresh consome o token em script atômico (GET + DEL +
  marcador de uso) e emite um novo par da mesma família. Reutilizar um token
  consumido revoga a família e devolve `401`.
- **Logout** apaga o token do Redis e a entrada de sessão do dispositivo.

### Revogação em massa

`token_version` no usuário é a alavanca:

| Evento | Efeito |
|---|---|
| troca de senha | `token_version++` → todos os refresh antigos viram inválidos |
| mudança de papel | `token_version++` → token com papel antigo deixa de valer |
| desativação | `active = false` + `token_version++` |

No refresh, além de consumir o token, o serviço confere se o `token_version`
armazenado ainda bate com o do banco. No access token, quem garante é
`require_session`.

### Troca da própria senha

`PUT /auth/me/password` exige a senha atual, recusa a nova igual à atual e
incrementa `token_version`. O efeito é deliberado: **todas** as sessões caem,
inclusive a que fez a troca, e o usuário faz login de novo. Um par novo emitido
na hora manteria viva uma sessão que pode estar comprometida; o custo de um
login extra é aceitável.

O aviso de troca vai para o outbox com o nome do usuário, nunca com a senha.

> O marcador de uso fica no Redis pelo mesmo TTL do refresh (7 dias). Reuso
> revoga a família inteira; um novo login cria outra família e segue válido.

## Recuperação de senha

- `POST /auth/password/forgot` devolve **204 sempre**, exista ou não a conta.
  Não há enumeração de e-mails; a única resposta diferente é `429` do rate limit
  (que independe de a conta existir).
- Código opaco de 64 caracteres hex, 30 minutos, guardado como **hash** no Redis
  (`auth:pwreset:<sha256>`), consumo atômico `GET` + `DEL`.
- O valor guardado carrega o `token_version` da emissão e um fingerprint do
  e-mail. Troca de senha, mudança de papel, desativação ou troca de e-mail
  invalidam códigos pendentes.
- A nova senha passa pela mesma política (12+ caracteres) e pelo mesmo Argon2id
  do login.
- `POST /auth/password/reset` incrementa `token_version` (derruba todas as
  sessões), confirma o e-mail (o código foi entregue nele), limpa os metadados
  de sessão e avisa por e-mail.
- Rate limit: 3 pedidos por conta e 10 por IP na janela, em chaves separadas das
  do login.
- Falha ao enfileirar o e-mail não muda a resposta: fica no log com o
  destinatário mascarado.

## Confirmação de e-mail

- Código opaco de 7 dias, hash no Redis, uso único.
- Emitido na criação do usuário e quando o e-mail muda (o e-mail antigo deixa de
  ser confiável; `email_verified_at` volta a nulo).
- `POST /auth/email/verify` é público; repetir um código já consumido devolve
  `400`, confirmar conta já confirmada devolve `204`.
- `POST /auth/email/resend` exige sessão e é limitado a 3 reenvios por usuário
  na janela; com e-mail já confirmado é no-op (`204`).
- `security.require_email_verified = true` faz o login devolver `403` enquanto o
  e-mail não for confirmado. Padrão `false`: o template não depende de SMTP para
  funcionar.
- O envio é best-effort: a conta é criada mesmo se Redis/SMTP falharem; o
  usuário pode pedir reenvio depois.

## Sessões por dispositivo

- Cada login é uma família de refresh; a família é o identificador de sessão
  (`fid` no access token).
- Metadados em `auth:session:<user_id>:<family_id>`: criação, último uso, IP e
  User-Agent (truncado em 200 caracteres), com o TTL do refresh.
- `GET /auth/sessions` lista da mais recente para a mais antiga, com `current`
  marcando a sessão da requisição.
- `DELETE /auth/sessions/{familyId}` remove a entrada **do próprio usuário**,
  revoga a família e devolve `404` se não existir. A sessão atual é recusada com
  `400`: encerrar a própria sessão é `POST /auth/logout`.
- Metadados são apagados no logout, no reset de senha, na troca de senha e na
  redefinição pelo admin; o `token_version` já invalida os tokens mesmo que a
  limpeza do Redis falhe.
- Reiniciar o Redis derruba sessões e códigos pendentes — consequência aceita do
  modelo efêmero.

## Bootstrap de admin

- Primeiro admin **só** por CLI: `just admin-create <email> "<nome>"`.
- Nunca por rota pública, nunca por seed com senha no repositório.
- `create` recusa se já existe admin ativo, a menos que `--force`.
- Senha por prompt sem eco ou `ADMIN_PASSWORD` (só automação).
- `reset-password` sobe o `token_version` e derruba as sessões do usuário.
- O CLI usa `load_unvalidated`: ele não deve exigir segredo JWT para redefinir
  senha.

## Segredos e configuração

- `APP__JWT__SECRET`: mínimo 32 caracteres, **só** por ambiente. Não tem default,
  e o boot da API/worker falha sem ele; `migrator` e `admin` não exigem segredo.
- O `Debug` de `Settings` redige o segredo; ele não aparece em log de boot.
- `.env` é apenas para `development` e está no `.gitignore`.
- Nunca logar token, senha, hash ou PII. Log de erro 5xx é genérico para o
  cliente; o detalhe fica no servidor com o `x-request-id`.
- Dependência nova: mantida, licença permitida, features mínimas e `cargo deny`
  verde.

## E-mail e outbox

- A senha nunca vai em e-mail, nem em template. O `welcome` não contém a senha.
- No provider `log`, o destinatário é mascarado (`a***@dominio`); assunto e
  remetente aparecem, corpo não.
- O corpo do e-mail fica no banco (`email_outbox`) até 30 dias depois do envio;
  não grave segredo, token ou dado sensível nele.
- `APP__MAIL__SMTP_URL` contém credenciais e é redigida no `Debug`; só no
  ambiente.
- O worker entrega at-least-once: não use e-mail como confirmação única de
  operação financeira sem chave de idempotência no provedor.
- Mailpit é apenas local (`127.0.0.1:1025/8025`), sem TLS nem autenticação, e
  não deve ser exposto nem usado em produção. Em produção, `smtps://` com
  credencial só no ambiente.

## Superfície HTTP

Já configurado em `core/http.rs` e `config`:

- limite de body (`server.body_limit_mb`);
- timeout de request (`server.request_timeout_secs` → 408);
- CORS por ambiente;
- `x-request-id` propagado/gerado;
- compressão;
- trace com `tracing`.

Erros saem em `application/problem+json` e 5xx nunca vaza `DbErr`/mensagem
interna.

## Ameaças e respostas

| Ameaça | Resposta atual |
|---|---|
| Força bruta de senha | Argon2id + rate limit por conta (5) e por IP (20) em 15 min; `429` com `Retry-After` |
| Token roubado (access) | TTL curto + `require_session` nas rotas sensíveis |
| Token roubado (refresh) | rotação atômica, hash no Redis, `token_version` e revogação de família no reuso |
| Repetição de `POST` | `Idempotency-Key` obrigatória em recurso, resposta gravada por 24 h |
| Escalada de privilégio | permissão checada no servidor; papel validado no banco |
| Abuso de conta admin | `audit_log` registra autor, ação, alvo, diff e `x-request-id` |
| Enumeração de contas | mesma resposta para e-mail inexistente/senha errada; recuperação devolve 204 sempre |
| Reset de senha por e-mail | código de uso único, hash no Redis, amarrado ao `token_version` |
| Sessão esquecida em dispositivo | `GET/DELETE /auth/sessions` com IP e User-Agent; logout revoga a família |
| Injeção SQL | SeaORM parametrizado; sem SQL concatenado |
| Vazamento em log | segredo redigido, 5xx genérico, sem PII |
| Auto-bloqueio do sistema | exclusão/rebaixamento do último admin bloqueado (fase 4) |

## Rate limit e reuso

- **Janela fixa** de 15 min, contadores no Redis, chaves com hash do e-mail/IP
  (sem PII no Redis). Login, recuperação (`auth:pwreset:fail:*`) e reenvio de
  confirmação (`auth:verify:fail:user:*`) usam chaves separadas.
- Bloqueio devolve `429` e `Retry-After`; login válido zera as falhas da conta,
  não as do IP.
- **Fail-open**: Redis indisponível não impede login; o evento vai para o log.
  A segunda barreira é o limite por IP no proxy.
- **Famílias de refresh**: cada login cria uma família; a rotação mantém a
  família. Reapresentar token já consumido revoga a família inteira. Um novo
  login (nova família) não é afetado.

## Idempotência

- `Idempotency-Key` é **obrigatória** em `POST` de recurso sob `/api/v1`; sem
  ela a resposta é `400`. A assinatura inclui método, caminho, `sub` do access
  token e a chave.
- A primeira execução grava status, `content-type`, `location`, `etag` e corpo
  por 24 h; repetição devolve a mesma resposta.
- `/api/v1/auth/*` fica fora: cachear resposta com tokens permitiria replay de
  refresh já rotacionado e derrubaria a família inteira.
- Enquanto a primeira execução está em andamento, a repetição recebe `409`.
- `5xx` não é gravado, para permitir retry.
- Redis fora do ar é fail-open: a requisição segue, mas sem garantia de
  idempotência.

## Auditoria

- `audit_log` é append-only (sem update/delete no código) e grava **na mesma
  transação** da mudança: se a alteração não commitou, o registro também não.
- Ações registradas: `user.created`, `user.admin_created` (CLI),
  `user.updated`, `user.role_changed`, `user.status_changed`,
  `user.password_reset`.
- `changes` guarda só o diff (`{ "role": { "from": "user", "to": "admin" } }`).
  Senha, hash e token nunca entram.
- `actor_id` nulo indica CLI ou job, não requisição HTTP.
- `GET /api/v1/audit` exige `audit:read` e é somente leitura. Ele valida o JWT,
  mas **não** confere `token_version`: um admin rebaixado consegue ler até o
  access token expirar (15 min). Aceitável para leitura, nunca para escrita.
- Não há FK de `actor_id`/`target_id` para `users`: a auditoria é um retrato
  histórico e não deve impedir ou acompanhar a remoção do usuário.
- Não existe expurgo automático. Defina a política de retenção antes de
  produção; apagar linha de auditoria é decisão explícita, fora da aplicação.

## Hardening pendente

- [ ] Cabeçalhos de segurança (HSTS, `X-Content-Type-Options`) no proxy.
- [ ] Rotação de segredo JWT com `kid` (só quando houver necessidade real).
- [x] Auditoria de ações administrativas (quem mudou o quê e quando).
- [x] Recuperação de senha por e-mail (código opaco de uso único).
- [x] Verificação de e-mail (opcional no login).
- [x] Sessões por dispositivo com revogação individual.
- [ ] Notificar login de dispositivo novo por e-mail.
- [ ] Exigir e-mail confirmado em produção (virar `true` depois de validar o SMTP).
