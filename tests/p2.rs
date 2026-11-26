//! P2: retenção da auditoria, limpeza de sessões e código anterior.

mod common;

use axum::http::StatusCode;
use chrono::{Duration, Utc};
use common::{Harness, bearer, session, verification_token};
use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, Set};
use serde_json::json;
use template_backend_rust::{
    database::entity::audit_log::{self, Column as AuditColumn, Entity as Audit},
    modules::{audit::service as audit_service, auth::service as auth_service},
};
use uuid::Uuid;

#[tokio::test]
async fn expurgo_da_auditoria_respeita_retencao() {
    let app = Harness::spawn().await;
    let now = Utc::now().fixed_offset();

    // O harness cria o admin, que já gera auditoria; isola o cenário.
    Audit::delete_many()
        .exec(&app.db)
        .await
        .expect("limpar auditoria inicial");

    let old_id = Uuid::now_v7();
    let recent_id = Uuid::now_v7();

    for (id, created_at) in [
        (old_id, now - Duration::days(60)),
        (recent_id, now - Duration::days(1)),
    ] {
        Audit::insert(audit_log::ActiveModel {
            id: Set(id),
            actor_id: Set(None),
            actor_role: Set(None),
            action: Set("user.updated".to_owned()),
            target_type: Set("user".to_owned()),
            target_id: Set(Uuid::now_v7()),
            changes: Set(None),
            request_id: Set(None),
            created_at: Set(created_at),
        })
        .exec(&app.db)
        .await
        .expect("inserir auditoria");
    }

    assert_eq!(
        audit_service::purge(&app.db, 30)
            .await
            .expect("expurgar auditoria"),
        1
    );
    assert_eq!(Audit::find().count(&app.db).await.expect("contagem"), 1);
    assert!(
        Audit::find_by_id(recent_id)
            .one(&app.db)
            .await
            .expect("buscar recente")
            .is_some()
    );

    // `0` desliga o expurgo.
    assert_eq!(
        audit_service::purge(&app.db, 0)
            .await
            .expect("expurgo desligado"),
        0
    );
    assert_eq!(
        Audit::find()
            .filter(AuditColumn::Action.eq("user.updated"))
            .count(&app.db)
            .await
            .expect("contagem por action"),
        1
    );
}

#[tokio::test]
async fn limpeza_de_sessoes_remove_metadados_do_redis() {
    let app = Harness::spawn().await;
    let user = app
        .create_user("Sessao", "sessao@teste.local", "SenhaDaSessao#2026")
        .await;
    let user_id = Uuid::parse_str(user["id"].as_str().expect("id")).expect("uuid");

    let login = session(&app.server, "sessao@teste.local", "SenhaDaSessao#2026").await;
    let access = login["accessToken"].as_str().expect("access");

    let before: serde_json::Value = app
        .server
        .get("/api/v1/auth/sessions")
        .add_header("authorization", bearer(access))
        .await
        .json();
    assert_eq!(before["items"].as_array().expect("itens").len(), 1);

    auth_service::clear_sessions(&app.cache, user_id).await;

    let after: serde_json::Value = app
        .server
        .get("/api/v1/auth/sessions")
        .add_header("authorization", bearer(access))
        .await
        .json();
    assert!(after["items"].as_array().expect("itens").is_empty());
}

#[tokio::test]
async fn reenvio_invalida_codigo_anterior() {
    let app = Harness::spawn().await;
    app.create_user("Davi", "davi@teste.local", "SenhaDoDavi#2026")
        .await;

    let old = verification_token(&app.db, "davi@teste.local").await;

    let resend = app
        .server
        .post("/api/v1/auth/email/resend")
        .json(&json!({ "email": "davi@teste.local" }))
        .await;
    resend.assert_status(StatusCode::NO_CONTENT);

    let new = verification_token(&app.db, "davi@teste.local").await;
    assert_ne!(old, new);

    let stale = app
        .server
        .post("/api/v1/auth/email/verify")
        .json(&json!({ "token": old }))
        .await;
    stale.assert_status(StatusCode::BAD_REQUEST);

    let fresh = app
        .server
        .post("/api/v1/auth/email/verify")
        .json(&json!({ "token": new }))
        .await;
    fresh.assert_status(StatusCode::NO_CONTENT);
}
