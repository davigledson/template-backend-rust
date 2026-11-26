//! Fluxo de API: autenticação, perfil, gestão de usuários, idempotência,
//! rate limit, OpenAPI e métricas.
//!
//! Um teste ordenado mantém os containers vivos e narra o fluxo completo; as
//! regras puras ficam nos testes unitários de `src`.

mod common;

use axum::http::StatusCode;
use common::{ADMIN_EMAIL, ADMIN_PASSWORD, Harness, bearer, session};
use serde_json::{Value, json};

#[tokio::test]
async fn fluxo_de_api() {
    let app = Harness::spawn().await;
    let server = &app.server;

    // --- sem token ---
    let response = server.get("/api/v1/users").await;
    response.assert_status(StatusCode::UNAUTHORIZED);
    assert!(
        response
            .content_type()
            .starts_with("application/problem+json")
    );

    let admin = app.login(ADMIN_EMAIL, ADMIN_PASSWORD).await;

    // --- criação exige Idempotency-Key ---
    let sem_chave = server
        .post("/api/v1/users")
        .add_header("authorization", bearer(&admin))
        .json(&json!({
            "name": "Ana",
            "email": "ana@teste.local",
            "password": "SenhaDaAna#2026",
            "role": "user"
        }))
        .await;
    sem_chave.assert_status(StatusCode::BAD_REQUEST);
    assert!(sem_chave.text().contains("Idempotency-Key"));

    let response = server
        .post("/api/v1/users")
        .add_header("authorization", bearer(&admin))
        .add_header("idempotency-key", "cria-ana")
        .json(&json!({
            "name": "Ana",
            "email": "ana@teste.local",
            "password": "SenhaDaAna#2026",
            "role": "user"
        }))
        .await;
    response.assert_status(StatusCode::CREATED);
    let ana_location = response
        .headers()
        .get("location")
        .expect("Location no create")
        .to_str()
        .expect("location legível")
        .to_owned();
    let created: Value = response.json();
    assert_eq!(created["email"], "ana@teste.local");
    assert_eq!(created["role"], "user");
    assert!(created.get("passwordHash").is_none());
    let ana_id = created["id"].as_str().expect("id da ana").to_owned();
    assert_eq!(ana_location, format!("/api/v1/users/{ana_id}"));

    // --- replay devolve a mesma resposta, inclusive o Location ---
    let replay = server
        .post("/api/v1/users")
        .add_header("authorization", bearer(&admin))
        .add_header("idempotency-key", "cria-ana")
        .json(&json!({
            "name": "Ana",
            "email": "ana@teste.local",
            "password": "SenhaDaAna#2026",
            "role": "user"
        }))
        .await;
    replay.assert_status(StatusCode::CREATED);
    assert_eq!(
        replay
            .headers()
            .get("location")
            .expect("Location no replay")
            .to_str()
            .expect("location legível"),
        ana_location
    );
    let replayed: Value = replay.json();
    assert_eq!(replayed["id"], ana_id.as_str());

    // --- login não passa por idempotência ---
    let login_key = "login-nao-e-cacheado";
    let first_login = server
        .post("/api/v1/auth/login")
        .add_header("idempotency-key", login_key)
        .json(&json!({ "email": ADMIN_EMAIL, "password": ADMIN_PASSWORD }))
        .await;
    first_login.assert_status_ok();
    let first_login: Value = first_login.json();

    let second_login = server
        .post("/api/v1/auth/login")
        .add_header("idempotency-key", login_key)
        .json(&json!({ "email": ADMIN_EMAIL, "password": ADMIN_PASSWORD }))
        .await;
    second_login.assert_status_ok();
    let second_login: Value = second_login.json();
    assert_ne!(
        first_login["refreshToken"], second_login["refreshToken"],
        "login repetido precisa gerar uma sessão nova"
    );

    // --- replay sobrevive à renovação do access token (fingerprint por `sub`) ---
    let renewed = second_login["accessToken"]
        .as_str()
        .expect("access renovado")
        .to_owned();
    assert_ne!(renewed, admin);

    let replay_com_token_novo = server
        .post("/api/v1/users")
        .add_header("authorization", bearer(&renewed))
        .add_header("idempotency-key", "cria-ana")
        .json(&json!({
            "name": "Ana",
            "email": "ana@teste.local",
            "password": "SenhaDaAna#2026",
            "role": "user"
        }))
        .await;
    replay_com_token_novo.assert_status(StatusCode::CREATED);
    let replayed: Value = replay_com_token_novo.json();
    assert_eq!(replayed["id"], ana_id.as_str());

    // --- conflito e chave inválida ---
    let duplicate = server
        .post("/api/v1/users")
        .add_header("authorization", bearer(&admin))
        .add_header("idempotency-key", "duplicada")
        .json(&json!({
            "name": "Outra",
            "email": "ANA@teste.local",
            "password": "OutraSenha#2026"
        }))
        .await;
    duplicate.assert_status(StatusCode::CONFLICT);

    let invalid_key = server
        .post("/api/v1/users")
        .add_header("authorization", bearer(&admin))
        .add_header("idempotency-key", "com espaco")
        .json(&json!({
            "name": "Curta",
            "email": "curta@teste.local",
            "password": "CurtaSenha#2026"
        }))
        .await;
    invalid_key.assert_status(StatusCode::BAD_REQUEST);

    let invalid = server
        .post("/api/v1/users")
        .add_header("authorization", bearer(&admin))
        .add_header("idempotency-key", "senha-curta")
        .json(&json!({
            "name": "Curta",
            "email": "curta@teste.local",
            "password": "curta"
        }))
        .await;
    invalid.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        invalid
            .content_type()
            .starts_with("application/problem+json")
    );

    // --- listagem e filtro ---
    let page: Value = server
        .get("/api/v1/users?search=ana&perPage=10")
        .add_header("authorization", bearer(&admin))
        .await
        .json();
    assert_eq!(page["total"], 1);
    assert_eq!(page["items"][0]["id"], ana_id.as_str());
    assert_eq!(page["perPage"], 10);

    let by_role: Value = server
        .get("/api/v1/users?role=admin&active=true")
        .add_header("authorization", bearer(&admin))
        .await
        .json();
    assert_eq!(by_role["total"], 1, "só o admin criado pelo harness");

    // --- usuário comum não gerencia usuários ---
    let ana = app.login("ana@teste.local", "SenhaDaAna#2026").await;
    let forbidden = server
        .get("/api/v1/users")
        .add_header("authorization", bearer(&ana))
        .await;
    forbidden.assert_status(StatusCode::FORBIDDEN);
    assert!(
        forbidden
            .content_type()
            .starts_with("application/problem+json")
    );

    // --- mudança de papel invalida a sessão antiga ---
    let promoted = server
        .patch(&format!("/api/v1/users/{ana_id}"))
        .add_header("authorization", bearer(&admin))
        .json(&json!({ "name": "Ana Silva", "role": "admin" }))
        .await;
    promoted.assert_status_ok();
    let promoted: Value = promoted.json();
    assert_eq!(promoted["name"], "Ana Silva");
    assert_eq!(promoted["role"], "admin");

    // --- `version` do cliente detecta edição concorrente ---
    let stale_version = created["version"].as_i64().expect("version inicial");
    let stale_patch = server
        .patch(&format!("/api/v1/users/{ana_id}"))
        .add_header("authorization", bearer(&admin))
        .json(&json!({
            "name": "Ana Versão Antiga",
            "version": stale_version
        }))
        .await;
    stale_patch.assert_status(StatusCode::CONFLICT);

    // --- PATCH sem mudança efetiva não incrementa a versão ---
    let current_version = promoted["version"].as_i64().expect("version atual");
    let noop_patch: Value = server
        .patch(&format!("/api/v1/users/{ana_id}"))
        .add_header("authorization", bearer(&admin))
        .json(&json!({
            "name": "Ana Silva",
            "version": current_version
        }))
        .await
        .json();
    assert_eq!(noop_patch["version"], current_version);

    let stale = server
        .get("/api/v1/auth/me")
        .add_header("authorization", bearer(&ana))
        .await;
    stale.assert_status(StatusCode::UNAUTHORIZED);

    let ana_admin = app.login("ana@teste.local", "SenhaDaAna#2026").await;
    let me: Value = server
        .get("/api/v1/auth/me")
        .add_header("authorization", bearer(&ana_admin))
        .await
        .json();
    assert_eq!(me["role"], "admin");

    // --- rebaixar (há outro admin) e desativar ---
    let demoted = server
        .patch(&format!("/api/v1/users/{ana_id}"))
        .add_header("authorization", bearer(&admin))
        .json(&json!({ "role": "user" }))
        .await;
    demoted.assert_status_ok();

    let deleted = server
        .delete(&format!("/api/v1/users/{ana_id}"))
        .add_header("authorization", bearer(&admin))
        .await;
    deleted.assert_status_no_content();

    // Soft delete é idempotente.
    let deleted_again = server
        .delete(&format!("/api/v1/users/{ana_id}"))
        .add_header("authorization", bearer(&admin))
        .await;
    deleted_again.assert_status_no_content();

    let inactive: Value = server
        .get(&format!("/api/v1/users/{ana_id}"))
        .add_header("authorization", bearer(&admin))
        .await
        .json();
    assert_eq!(inactive["active"], false);

    // --- redefinir senha e reativar ---
    let password = server
        .put(&format!("/api/v1/users/{ana_id}/password"))
        .add_header("authorization", bearer(&admin))
        .json(&json!({ "password": "SenhaNova#2026" }))
        .await;
    password.assert_status_no_content();

    let reactivated = server
        .patch(&format!("/api/v1/users/{ana_id}"))
        .add_header("authorization", bearer(&admin))
        .json(&json!({ "active": true }))
        .await;
    reactivated.assert_status_ok();

    let old_login = server
        .post("/api/v1/auth/login")
        .json(&json!({ "email": "ana@teste.local", "password": "SenhaDaAna#2026" }))
        .await;
    old_login.assert_status(StatusCode::UNAUTHORIZED);

    let ana_session = session(server, "ana@teste.local", "SenhaNova#2026").await;
    let ana_token = ana_session["accessToken"]
        .as_str()
        .expect("access token da ana")
        .to_owned();
    let ana_refresh = ana_session["refreshToken"]
        .as_str()
        .expect("refresh token da ana")
        .to_owned();

    // --- perfil do próprio usuário ---
    let patched: Value = server
        .patch("/api/v1/auth/me")
        .add_header("authorization", bearer(&ana_token))
        .json(&json!({ "name": "Ana Souza" }))
        .await
        .json();
    assert_eq!(patched["name"], "Ana Souza");

    let me_ana: Value = server
        .get("/api/v1/auth/me")
        .add_header("authorization", bearer(&ana_token))
        .await
        .json();
    assert_eq!(me_ana["name"], "Ana Souza");

    let short = server
        .put("/api/v1/auth/me/password")
        .add_header("authorization", bearer(&ana_token))
        .json(&json!({
            "currentPassword": "SenhaNova#2026",
            "newPassword": "curta"
        }))
        .await;
    short.assert_status(StatusCode::UNPROCESSABLE_ENTITY);

    let wrong_current = server
        .put("/api/v1/auth/me/password")
        .add_header("authorization", bearer(&ana_token))
        .json(&json!({
            "currentPassword": "senha-errada#2026",
            "newPassword": "SenhaFinal#2026"
        }))
        .await;
    wrong_current.assert_status(StatusCode::UNAUTHORIZED);

    let same_password = server
        .put("/api/v1/auth/me/password")
        .add_header("authorization", bearer(&ana_token))
        .json(&json!({
            "currentPassword": "SenhaNova#2026",
            "newPassword": "SenhaNova#2026"
        }))
        .await;
    same_password.assert_status(StatusCode::BAD_REQUEST);

    let changed = server
        .put("/api/v1/auth/me/password")
        .add_header("authorization", bearer(&ana_token))
        .json(&json!({
            "currentPassword": "SenhaNova#2026",
            "newPassword": "SenhaFinal#2026"
        }))
        .await;
    changed.assert_status_no_content();

    // A troca derruba inclusive a sessão que a fez.
    let stale_after_change = server
        .get("/api/v1/auth/me")
        .add_header("authorization", bearer(&ana_token))
        .await;
    stale_after_change.assert_status(StatusCode::UNAUTHORIZED);

    let stale_refresh = server
        .post("/api/v1/auth/refresh")
        .json(&json!({ "refreshToken": ana_refresh }))
        .await;
    stale_refresh.assert_status(StatusCode::UNAUTHORIZED);

    let old_password = server
        .post("/api/v1/auth/login")
        .json(&json!({ "email": "ana@teste.local", "password": "SenhaNova#2026" }))
        .await;
    old_password.assert_status(StatusCode::UNAUTHORIZED);

    let final_session = session(server, "ana@teste.local", "SenhaFinal#2026").await;
    let final_access = final_session["accessToken"]
        .as_str()
        .expect("access final")
        .to_owned();
    let first_refresh = final_session["refreshToken"]
        .as_str()
        .expect("refresh inicial")
        .to_owned();

    // --- rotação e detecção de reuso de refresh token ---
    let rotated = server
        .post("/api/v1/auth/refresh")
        .json(&json!({ "refreshToken": first_refresh }))
        .await;
    rotated.assert_status_ok();
    let rotated: Value = rotated.json();
    let second_refresh = rotated["refreshToken"]
        .as_str()
        .expect("refresh rotacionado")
        .to_owned();
    assert_ne!(first_refresh, second_refresh);

    let reuse = server
        .post("/api/v1/auth/refresh")
        .json(&json!({ "refreshToken": first_refresh }))
        .await;
    reuse.assert_status(StatusCode::UNAUTHORIZED);

    // O reuso revoga a família inteira: o token rotacionado também cai.
    let family_revoked = server
        .post("/api/v1/auth/refresh")
        .json(&json!({ "refreshToken": second_refresh }))
        .await;
    family_revoked.assert_status(StatusCode::UNAUTHORIZED);

    // O access token segue válido até expirar (JWT stateless).
    let access_still_valid = server
        .get("/api/v1/auth/me")
        .add_header("authorization", bearer(&final_access))
        .await;
    access_still_valid.assert_status_ok();

    // Uma família nova, de outro login, não é afetada.
    let other_session = session(server, "ana@teste.local", "SenhaFinal#2026").await;
    let other_refresh = other_session["refreshToken"]
        .as_str()
        .expect("refresh da outra sessão")
        .to_owned();
    let other_rotated = server
        .post("/api/v1/auth/refresh")
        .json(&json!({ "refreshToken": other_refresh }))
        .await;
    other_rotated.assert_status_ok();

    // --- rate limit de login ---
    for _ in 0..5 {
        let failure = server
            .post("/api/v1/auth/login")
            .json(&json!({
                "email": "rate.limit@teste.local",
                "password": "SenhaErrada#2026"
            }))
            .await;
        failure.assert_status(StatusCode::UNAUTHORIZED);
    }

    let blocked = server
        .post("/api/v1/auth/login")
        .json(&json!({
            "email": "rate.limit@teste.local",
            "password": "SenhaErrada#2026"
        }))
        .await;
    blocked.assert_status(StatusCode::TOO_MANY_REQUESTS);
    assert!(blocked.headers().get("retry-after").is_some());
    assert!(
        blocked
            .content_type()
            .starts_with("application/problem+json")
    );

    // --- proteções do próprio admin ---
    let admin_me: Value = server
        .get("/api/v1/auth/me")
        .add_header("authorization", bearer(&admin))
        .await
        .json();
    let admin_id = admin_me["id"].as_str().expect("id do admin");

    let self_delete = server
        .delete(&format!("/api/v1/users/{admin_id}"))
        .add_header("authorization", bearer(&admin))
        .await;
    self_delete.assert_status(StatusCode::CONFLICT);

    let empty_patch = server
        .patch(&format!("/api/v1/users/{ana_id}"))
        .add_header("authorization", bearer(&admin))
        .json(&json!({}))
        .await;
    empty_patch.assert_status(StatusCode::BAD_REQUEST);

    // --- auditoria ---
    let audit: Value = server
        .get(&format!("/api/v1/audit?targetId={ana_id}"))
        .add_header("authorization", bearer(&admin))
        .await
        .json();
    assert_eq!(audit["total"], 6, "{audit}");

    let created_event: Value = server
        .get(&format!(
            "/api/v1/audit?targetId={ana_id}&action=user.created"
        ))
        .add_header("authorization", bearer(&admin))
        .await
        .json();
    assert_eq!(created_event["total"], 1);
    assert_eq!(created_event["items"][0]["actorId"], admin_id);
    assert_eq!(created_event["items"][0]["actorRole"], "admin");
    assert!(created_event["items"][0]["requestId"].is_string());
    assert_eq!(created_event["items"][0]["changes"]["role"], "user");
    assert!(
        created_event["items"][0]["changes"]
            .get("password")
            .is_none()
    );

    let role_events: Value = server
        .get(&format!(
            "/api/v1/audit?targetId={ana_id}&action=user.role_changed"
        ))
        .add_header("authorization", bearer(&admin))
        .await
        .json();
    assert_eq!(role_events["total"], 2);

    let window: Value = server
        .get("/api/v1/audit?to=2020-01-01T00:00:00Z")
        .add_header("authorization", bearer(&admin))
        .await
        .json();
    assert_eq!(window["total"], 0);

    let bad_action = server
        .get("/api/v1/audit?action=user.inventado")
        .add_header("authorization", bearer(&admin))
        .await;
    bad_action.assert_status(StatusCode::BAD_REQUEST);
    assert!(
        bad_action
            .content_type()
            .starts_with("application/problem+json")
    );

    let forbidden_audit = server
        .get("/api/v1/audit")
        .add_header("authorization", bearer(&final_access))
        .await;
    forbidden_audit.assert_status(StatusCode::FORBIDDEN);

    // O token antigo da Ana ainda é um JWT válido e diz "admin", mas a sessão
    // foi rebaixada e a senha redefinida (token_version mudou): a rota confere
    // o banco, não só o token.
    let revoked_audit = server
        .get("/api/v1/audit")
        .add_header("authorization", bearer(&ana_admin))
        .await;
    revoked_audit.assert_status(StatusCode::UNAUTHORIZED);

    // --- documentação ---
    let spec: Value = server.get("/api-docs/openapi.json").await.json();
    assert!(spec["paths"]["/api/v1/users"].is_object());
    assert!(spec["paths"]["/api/v1/auth/me"]["patch"].is_object());
    assert!(spec["paths"]["/api/v1/auth/me/password"]["put"].is_object());
    assert!(spec["paths"]["/api/v1/audit"]["get"].is_object());
    assert!(spec["components"]["securitySchemes"]["bearer"].is_object());

    let docs = server.get("/docs").await;
    docs.assert_status_ok();
    assert!(docs.text().contains("Scalar"));

    // --- métricas ---
    let metrics = server.get("/metrics").await;
    metrics.assert_status_ok();
    let metrics = metrics.text();
    assert!(metrics.contains("axum_http_requests_total"), "{metrics}");
    assert!(metrics.contains("app_login_failure_total"), "{metrics}");
    assert!(
        metrics.contains("app_login_blocked_total{scope=\"account\"} 1"),
        "{metrics}"
    );
    assert!(metrics.contains("app_refresh_reuse_total 1"), "{metrics}");
    assert!(
        metrics.contains("app_idempotency_replay_total 2"),
        "{metrics}"
    );
}
