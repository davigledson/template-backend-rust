# Renomear o projeto

O template nasce com o nome `template-backend-rust`. Para começar um projeto de
cliente, use o rename em vez de editar os identificadores à mão.

## Uso

```bash
just rename acme-backend --dry-run   # mostra o que mudaria
just rename acme-backend             # pede confirmação
just rename acme-backend --yes       # sem prompt
```

O nome pode vir como `acme-backend`, `acme_backend` ou `"Acme Backend"`; o script
normaliza para:

| Identificador | Valor |
|---|---|
| Cargo, Docker, CI (kebab) | `acme-backend` |
| Caminhos Rust (snake) | `acme_backend` |
| Título (README/OpenAPI) | `Acme Backend` |
| JWT issuer/audience | `acme-backend-api` / `acme-backend-front` |

## O que muda

- `Cargo.toml` e `Cargo.lock` (o lock é regenerado pelo `cargo check`);
- `use <snake>::` em `src/main.rs`, `src/bin/*` e `tests/*`;
- `RUST_LOG` em `.env` e `.env.example`;
- `name:` dos três composes (`local`, `deploy`, `test`);
- tags de imagem no `justfile` e em `.github/workflows/production.yml`;
- título do OpenAPI/Scalar e o snapshot do insta (arquivo renomeado);
- `issuer`/`audience` do JWT em `default.toml` e `src/config/settings/jwt.rs`;
- título do `README.md` e do `AGENTS.md`.

## O que não muda

- a prosa que fala “template” (`docs/decisions.md`, `src/lib.rs`, `AGENTS.md`);
- `description` do `Cargo.toml`, licença, autoria e remote do git;
- histórico do git, versão (`0.1.0`) e migrations.

## Requisitos e efeitos

- Árvore git limpa: o rename usa `git mv` e exige que nada esteja pendente.
- O script roda `cargo check`, `cargo fmt` e os testes da lib ao final.
- Depois do rename, **tokens antigos deixam de valer** (issuer/audience
  mudaram): gere um `APP__JWT__SECRET` novo e faça login de novo.

## Checklist depois do rename

- [ ] revisar `description` no `Cargo.toml` e a prosa “template” nos docs;
- [ ] gerar `APP__JWT__SECRET` novo;
- [ ] configurar remote, CI e segredos do deploy;
- [ ] `just up && just migrate` e criar o primeiro admin;
- [ ] revisar `git diff` e commitar o rename.

## Desfazer

A árvore estava limpa, então `git reset --hard` volta ao estado anterior
(inclusive o snapshot com o nome antigo). O `RUST_LOG` do `.env` não é
versionado; se quiser, ajuste à mão.
