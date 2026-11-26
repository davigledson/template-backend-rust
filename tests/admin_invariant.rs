//! Invariante: nunca ficar sem administrador ativo sob concorrência.

mod common;

use common::{ADMIN_EMAIL, Harness};
use template_backend_rust::{
    core::{auth::role::Role, error::AppError},
    modules::{
        audit::service::Actor,
        users::service::{self as users_service, NewUser, UserUpdate},
    },
};

const SECOND_EMAIL: &str = "segundo.admin@teste.local";
const SECOND_PASSWORD: &str = "SegundoAdmin#2026";

#[tokio::test]
async fn dois_admins_nao_conseguem_se_rebaixar_ao_mesmo_tempo() {
    let app = Harness::spawn().await;

    let second = users_service::create(
        &app.db,
        &app.cache,
        Actor::system(),
        NewUser {
            name: "Segundo Admin".to_owned(),
            email: SECOND_EMAIL.to_owned(),
            password: SECOND_PASSWORD.to_owned(),
            role: Role::Admin,
        },
    )
    .await
    .expect("segundo admin");

    let first = users_service::find_by_email(&app.db, ADMIN_EMAIL)
        .await
        .expect("busca do primeiro")
        .expect("primeiro admin");

    // Bypassa o HTTP: os dois atores continuam válidos e a única barreira é a
    // regra do último admin (o lock da transação).
    let demote_second = users_service::update(
        &app.db,
        &app.cache,
        Actor::system(),
        second.id,
        UserUpdate {
            role: Some(Role::User),
            ..UserUpdate::default()
        },
    );
    let demote_first = users_service::update(
        &app.db,
        &app.cache,
        Actor::system(),
        first.id,
        UserUpdate {
            role: Some(Role::User),
            ..UserUpdate::default()
        },
    );

    let (first_result, second_result) = tokio::join!(demote_second, demote_first);

    let results = [first_result, second_result];
    assert_eq!(
        results.iter().filter(|result| result.is_ok()).count(),
        1,
        "exatamente uma alteração deveria passar: {results:?}"
    );
    assert!(
        results
            .iter()
            .any(|result| matches!(result, Err(AppError::Conflict(_)))),
        "a outra deveria bater no último admin: {results:?}"
    );

    let active_admins = users_service::count_active_admins(&app.db)
        .await
        .expect("contagem de admins");
    assert_eq!(active_admins, 1, "o banco não pode ficar sem admin ativo");
}
