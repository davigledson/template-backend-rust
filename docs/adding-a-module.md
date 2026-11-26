# Adicionando um módulo

Tutorial com o código real do template. Exemplo: um recurso `notes`
(`GET/POST /api/v1/notes`), que pertence ao usuário autenticado.

> Regra de bolso: se o módulo não couber em uma pasta, ele está grande demais.

## Atalho: gerar o esqueleto

```bash
just module notes            # cria src/modules/notes e registra no router
just module notas_fiscais --dry-run
```

O gerador cria `mod.rs`, `controller.rs`, `service.rs`, `repository.rs` e
`dto.rs`, registra o módulo em `src/modules/mod.rs`, adiciona `.merge()` no
router e atualiza o snapshot do OpenAPI. A rota nasce autenticada e responde
`501` até você trocá-la pela regra real. Nome com hífen/underscore/espaço é
normalizado para `snake_case`; a rota usa hífen (`/notas-fiscais`).

Depois do esqueleto, siga a migration e o restante do tutorial.

## 0. Antes de escrever código

Responda no papel:

- Qual tabela este módulo possui? (só ele escreve nela)
- Quais rotas existem e quais permissões cada uma exige?
- Qual o contrato JSON de entrada e saída?
- O `POST` cria recurso? Então exige `Idempotency-Key` e devolve `Location`.
- Quem mais precisa dos dados? (esses módulos chamarão o **service**)

## 1. Migration

Gere o arquivo e o registro:

```bash
just migration create_notes
```

Isso cria `src/migrations/mYYYYMMDD_HHMMSS_create_notes.rs` e adiciona a migration
em `src/migrations/mod.rs` e em `Migrator::migrations()`. Preencha `up` e
`down`; o template vem com `todo!()`.

O CLI serve só para gerar. Para aplicar, use `just migrate`: os comandos
`sea-orm-cli migrate up/status/down` esperam uma migration crate separada.

```rust
use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Notes::Table)
                    .col(pk_uuid(Notes::Id))
                    .col(uuid(Notes::OwnerId))
                    .col(string_len(Notes::Title, 200))
                    .col(text(Notes::Body))
                    .col(timestamp_with_time_zone_default_now(Notes::CreatedAt))
                    .col(timestamp_with_time_zone_default_now(Notes::UpdatedAt))
                    .foreign_key(
                        ForeignKey::create()
                            .from(Notes::Table, Notes::OwnerId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // Índice para o filtro que a listagem realmente usa.
        manager
            .create_index(
                Index::create()
                    .name("notes_owner_created_idx")
                    .table(Notes::Table)
                    .col(Notes::OwnerId)
                    .col(Notes::CreatedAt)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Notes::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Notes {
    Table,
    Id,
    OwnerId,
    Title,
    Body,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}
```

```bash
just migrate
just entities   # regenera src/database/entity/ (não edite à mão)
```

Se a entidade gerada diferir do esperado, o problema está na migration — não
"conserte" editando a entidade.

## 2. Estrutura do módulo

```text
src/modules/notes/
├── mod.rs
├── controller.rs       # ou controller/{mod,collection,item}.rs
├── dto.rs              # ou dto/{mod,list,output,write}.rs
├── repository.rs
└── service.rs          # ou service/{mod,lookup,create,update}.rs
```

Comece com arquivo único. Ao passar dos alvos (controller 120, service 150,
repository 150, dto 80 linhas), quebre por sub-recurso como em
`modules/users`; o `mod.rs` continua reexportando a API pública.

```rust
// src/modules/notes/mod.rs
pub mod controller;
pub mod dto;
mod repository;
pub mod service;

pub use controller::router;
```

## 3. DTO

Validação fica aqui, nunca no repository.

```rust
// src/modules/notes/dto.rs
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

use crate::core::error::AppError;
use crate::database::entity::notes;

#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreateNote {
    #[validate(length(min = 1, max = 200))]
    pub title: String,
    #[validate(length(max = 10_000))]
    pub body: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteOut {
    pub id: Uuid,
    pub title: String,
    pub body: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl NoteOut {
    pub fn from_model(model: notes::Model) -> Result<Self, AppError> {
        Ok(Self {
            id: model.id,
            title: model.title,
            body: model.body,
            created_at: model.created_at.with_timezone(&Utc),
            updated_at: model.updated_at.with_timezone(&Utc),
        })
    }
}
```

Nunca devolva a entidade direto: ela expõe colunas internas e amarra o contrato
ao schema.

## 4. Repository

Só query. Recebe a conexão, não o estado inteiro. Sempre filtre pelo dono quando
o recurso é do usuário.

```rust
// src/modules/notes/repository.rs
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, DbErr, EntityTrait,
    PaginatorTrait, QueryFilter, QueryOrder,
};
use uuid::Uuid;

use crate::database::entity::notes::{self, ActiveModel, Column, Entity};

pub async fn list_by_owner(
    db: &DatabaseConnection,
    owner_id: Uuid,
    offset: u64,
    limit: u64,
) -> Result<(Vec<notes::Model>, u64), DbErr> {
    let base = Entity::find().filter(Column::OwnerId.eq(owner_id));
    let total = base.clone().count(db).await?;
    let items = base
        .order_by_desc(Column::CreatedAt)
        .offset(offset)
        .limit(limit)
        .all(db)
        .await?;
    Ok((items, total))
}

pub async fn insert(db: &DatabaseConnection, note: ActiveModel) -> Result<notes::Model, DbErr> {
    note.insert(db).await
}
```

## 5. Service

Regra, transação e autorização de domínio. Não conhece HTTP.

```rust
// src/modules/notes/service.rs
use chrono::Utc;
use sea_orm::{DatabaseConnection, Set};
use uuid::Uuid;

use crate::core::auth::extractor::AuthUser;
use crate::core::error::AppError;
use crate::modules::notes::{dto::CreateNote, repository};

pub async fn create(
    db: &DatabaseConnection,
    auth: &AuthUser,
    input: CreateNote,
) -> Result<crate::database::entity::notes::Model, AppError> {
    let now = Utc::now().fixed_offset();
    let model = crate::database::entity::notes::ActiveModel {
        id: Set(Uuid::now_v7()),
        owner_id: Set(auth.id),
        title: Set(input.title.trim().to_owned()),
        body: Set(input.body),
        created_at: Set(now),
        updated_at: Set(now),
    };

    Ok(repository::insert(db, model).await?)
}
```

Se duas escritas precisam ser atômicas, abra a transação no **service** e passe o
`&DatabaseTransaction` ao repository.

## 6. Controller

Extrai, valida, checa permissão, chama service, responde. A rota é registrada
com `routes!` — o mesmo registro serve o Axum e o OpenAPI.

```rust
// src/modules/notes/controller.rs
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::core::auth::{extractor::AuthUser, permission::PROFILE_WRITE};
use crate::core::{
    error::AppError,
    extract::{ValidatedJson, ValidatedQuery},
    openapi::Problem,
    response::{Page, PageQuery},
    state::AppState,
};
use crate::modules::notes::{
    dto::{CreateNote, NoteOut},
    service,
};

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(list, create))
}

#[utoipa::path(
    get,
    path = "/notes",
    tag = "notes",
    params(PageQuery),
    security(("bearer" = [])),
    responses(
        (status = 200, body = Page<NoteOut>),
        (status = 401, body = Problem),
        (status = 403, body = Problem),
    )
)]
async fn list(
    State(state): State<AppState>,
    auth: AuthUser,
    ValidatedQuery(page): ValidatedQuery<PageQuery>,
) -> Result<Json<Page<NoteOut>>, AppError> {
    auth.require(PROFILE_WRITE)?;

    let (items, total) = service::list(&state.db, &auth, &page).await?;
    let items = items
        .into_iter()
        .map(NoteOut::from_model)
        .collect::<Result<Vec<_>, _>>()?;

    Ok(Json(Page::new(items, page.page, page.per_page(), total)))
}

#[utoipa::path(
    post,
    path = "/notes",
    tag = "notes",
    request_body = CreateNote,
    security(("bearer" = [])),
    responses(
        (status = 201, body = NoteOut),
        (status = 422, body = Problem),
    )
)]
async fn create(
    State(state): State<AppState>,
    auth: AuthUser,
    ValidatedJson(body): ValidatedJson<CreateNote>,
) -> Result<(StatusCode, Json<NoteOut>), AppError> {
    auth.require(PROFILE_WRITE)?;

    let model = service::create(&state.db, &auth, body).await?;
    Ok((StatusCode::CREATED, Json(NoteOut::from_model(model)?)))
}
```

`ValidatedJson`/`ValidatedQuery` transformam falha de validação em
`422 application/problem+json`; o `Json`/`Query` do Axum devolveria texto puro.

Não use `if auth.role == Role::Admin`: peça a permissão. Se a permissão ainda não
existe, adicione ao catálogo em `core/auth/permission.rs` **e** ao mapa do papel.

Rotas que precisam conferir `active`, `token_version` e papel no banco chamam
`users::service::require_session(&state.db, &auth).await?` (é o que o módulo
`users` faz). A leitura de `/audit` também chama, no controller, antes da
consulta.

## 7. Registrar o módulo

O path do `#[utoipa::path]` já inclui o prefixo do módulo (`/notes`), então o
registro é um `merge`:

```rust
// src/modules/mod.rs
pub mod auth;
pub mod notes;
pub mod users;

use utoipa_axum::router::OpenApiRouter;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .merge(auth::router())
        .merge(notes::router())
        .merge(users::router())
}
```

O `/api/v1` é aplicado uma única vez em `routes/openapi.rs`, que também gera o
`/api-docs/openapi.json` e serve o Scalar em `/docs`.

## 8. Testes

- Regra de service: teste unitário no próprio arquivo (`#[cfg(test)]`).
- Endpoint: teste de integração com `Harness` em `tests/`.
- Autorização: um teste provando 401 sem token e 403 sem permissão.
- `POST` de recurso sem `Idempotency-Key` → `400`; com a chave, repetir devolve a
  mesma resposta (inclusive `Location`) sem duplicar.
- Ação privilegiada grava auditoria na mesma transação
  (`audit::service::record_user`), com ação do vocabulário fechado e diff sem
  segredo.
- Token/segredo de uso único vive no Redis com hash, TTL e consumo atômico;
  nunca no banco em claro. O usuário precisa poder pedir outro.

```bash
just fmt && just lint && just test
```

## 9. OpenAPI e docs

Cada handler tem `#[utoipa::path]`; o `routes!` já entrega a rota e o schema. O
documento final (`/api-docs/openapi.json`, UI em `/docs`) é montado em
`routes/openapi.rs`, e um teste garante que as rotas aparecem. Se o módulo muda o
contrato público, atualize `README.md` e `docs/`.

O contrato tem snapshot com `insta` (`just openapi-check`): mudança intencional
exige atualizar o `.snap` e revisar o diff.

## Checklist final

- [ ] Migration + `just entities`
- [ ] Índice para os filtros reais
- [ ] DTO com validação
- [ ] Repository só com query
- [ ] Service com regra/transação
- [ ] Controller fino + permissão
- [ ] Rota registrada em `modules/mod.rs`
- [ ] Erros em Problem Details
- [ ] Testes unitários e de integração
- [ ] `just ci` verde
- [ ] OpenAPI/docs atualizados
