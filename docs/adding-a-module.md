# Adicionando um módulo

Tutorial com o código real do template. Exemplo: um recurso `notes`
(`GET/POST /api/v1/notes`), que pertence ao usuário autenticado.

> Regra de bolso: se o módulo não couber em uma pasta, ele está grande demais.

## 0. Antes de escrever código

Responda no papel:

- Qual tabela este módulo possui? (só ele escreve nela)
- Quais rotas existem e quais permissões cada uma exige?
- Qual o contrato JSON de entrada e saída?
- O `POST` é idempotente? Precisa de `Idempotency-Key`?
- Quem mais precisa dos dados? (esses módulos chamarão o **service**)

## 1. Migration

Crie `src/migrations/mYYYYMMDD_HHMMSS_create_notes.rs`. O nome do arquivo é o nome
da migration; registre em `src/migrations/mod.rs`.

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
├── controller.rs
├── dto.rs
├── repository.rs
└── service.rs
```

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

Extrai, valida, checa permissão, chama service, responde.

```rust
// src/modules/notes/controller.rs
use axum::{
    Json, Router,
    extract::{Query, State},
    http::StatusCode,
    routing::{get, post},
};
use axum_valid::Valid;

use crate::core::auth::{extractor::AuthUser, permission::PROFILE_WRITE};
use crate::core::{
    error::AppError,
    response::{Page, PageQuery},
    state::AppState,
};
use crate::modules::notes::{
    dto::{CreateNote, NoteOut},
    service,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list).post(create))
}

async fn list(
    State(state): State<AppState>,
    auth: AuthUser,
    Valid(Query(page)): Valid<Query<PageQuery>>,
) -> Result<Json<Page<NoteOut>>, AppError> {
    auth.require(PROFILE_WRITE)?;

    let (items, total) = service::list(&state.db, &auth, &page).await?;
    let items = items
        .into_iter()
        .map(NoteOut::from_model)
        .collect::<Result<Vec<_>, _>>()?;

    Ok(Json(Page::new(items, page.page, page.per_page(), total)))
}

async fn create(
    State(state): State<AppState>,
    auth: AuthUser,
    Valid(Json(body)): Valid<Json<CreateNote>>,
) -> Result<(StatusCode, Json<NoteOut>), AppError> {
    auth.require(PROFILE_WRITE)?;

    let model = service::create(&state.db, &auth, body).await?;
    Ok((StatusCode::CREATED, Json(NoteOut::from_model(model)?)))
}
```

Não use `if auth.role == Role::Admin`: peça a permissão. Se a permissão ainda não
existe, adicione ao catálogo em `core/auth/permission.rs` **e** ao mapa do papel.

## 7. Registrar o módulo

```rust
// src/modules/mod.rs
pub mod auth;
pub mod notes;
pub mod users;

pub fn router() -> Router<AppState> {
    Router::new()
        .nest("/auth", auth::router())
        .nest("/notes", notes::router())
}
```

## 8. Testes

- Regra de service: teste unitário no próprio arquivo (`#[cfg(test)]`).
- Endpoint: teste de integração com testcontainers em `tests/`.
- Autorização: um teste provando 401 sem token e 403 sem permissão.
- Todo `POST` com `Idempotency-Key`: teste de repetição não duplicando.

```bash
just fmt && just lint && just test
```

## 9. OpenAPI e docs

Na fase 4, cada handler ganha `#[utoipa::path]` e o módulo entra no documento.
Atualize o snapshot no mesmo PR (`just openapi-check`). Se o módulo muda o
contrato público, atualize `README.md` e `docs/`.

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
