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
| Claims | `sub`, `email`, `role`, `token_version`, `jti`, `iat`, `exp`, `iss`, `aud` |
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
- Valor armazenado: `"{user_id}:{token_version}"`.
- TTL de 7 dias.
- **Rotação**: cada refresh consome o token com `GETDEL` (atômico) e emite um
  novo par. Reutilizar o token antigo → `401`.
- **Logout** apaga o token do Redis.

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

> **Pendente (fase 7)**: detecção de reuso de refresh token com revogação da
> família. Hoje, reusar um token já consumido só falha; não derruba as sessões
> derivadas dele.

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
  e o boot falha sem ele.
- O `Debug` de `Settings` redige o segredo; ele não aparece em log de boot.
- `.env` é apenas para `development` e está no `.gitignore`.
- Nunca logar token, senha, hash ou PII. Log de erro 5xx é genérico para o
  cliente; o detalhe fica no servidor com o `x-request-id`.
- Dependência nova: mantida, licença permitida, features mínimas e `cargo deny`
  verde.

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
| Força bruta de senha | Argon2id + mensagem uniforme; **rate limit pendente** |
| Token roubado (access) | TTL curto + `require_session` nas rotas sensíveis |
| Token roubado (refresh) | rotação com `GETDEL`, hash no Redis, revogação por `token_version` |
| Escalada de privilégio | permissão checada no servidor; papel validado no banco |
| Enumeração de contas | mesma resposta para e-mail inexistente/senha errada |
| Injeção SQL | SeaORM parametrizado; sem SQL concatenado |
| Vazamento em log | segredo redigido, 5xx genérico, sem PII |
| Auto-bloqueio do sistema | exclusão/rebaixamento do último admin bloqueado (fase 4) |

## Hardening pendente

- [ ] Rate limit no login (`tower_governor`) e por IP/conta.
- [ ] Detecção de reuso de refresh token (família) com revogação.
- [ ] `Idempotency-Key` nas rotas `POST`.
- [ ] Cabeçalhos de segurança (HSTS, `X-Content-Type-Options`) no proxy.
- [ ] Rotação de segredo JWT com `kid` (só quando houver necessidade real).
- [ ] Auditoria de ações administrativas (quem mudou o quê e quando).
