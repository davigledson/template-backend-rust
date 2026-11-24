//! Fluxo do outbox de e-mail: entrega, deduplicação e retry com backoff.
//!
//! Não depende de SMTP: usa `RecordingMailer`/`FailingMailer`, como o worker
//! faria com qualquer implementação de `Mailer`.

mod common;

use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, sea_query::Expr};
use template_backend_rust::{
    core::mail::EmailMessage,
    database::entity::email_outbox::{Column as OutboxColumn, Entity as Outbox},
    modules::email::service as email_service,
};

use common::{FailingMailer, Harness, RecordingMailer};

#[tokio::test]
async fn fluxo_do_outbox_de_email() {
    let app = Harness::spawn().await;
    let db = &app.db;

    app.create_user("Ana", "ana@teste.local", "SenhaDaAna#2026")
        .await;

    // --- entrega ---
    let pending = Outbox::find()
        .filter(OutboxColumn::Status.eq("pending"))
        .count(db)
        .await
        .expect("contagem do outbox");
    assert_eq!(pending, 2, "admin + ana");

    let recording = RecordingMailer::default();
    let summary = email_service::process_pending(db, &recording, 10)
        .await
        .expect("processar outbox");
    assert_eq!(summary.claimed, 2);
    assert_eq!(summary.sent, 2);
    assert_eq!(summary.failed, 0);

    let subjects = recording.subjects();
    assert_eq!(
        subjects
            .iter()
            .filter(|subject| *subject == "Sua conta foi criada")
            .count(),
        2
    );

    let sent = Outbox::find()
        .filter(OutboxColumn::Status.eq("sent"))
        .count(db)
        .await
        .expect("enviados");
    assert_eq!(sent, 2);

    // --- deduplicação pelo dedupe_key ---
    let message = EmailMessage {
        to: "dedupe@teste.local".to_owned(),
        subject: "Teste".to_owned(),
        body: "corpo".to_owned(),
    };
    email_service::enqueue(db, message.clone(), "manual-dedupe".to_owned())
        .await
        .expect("primeiro enqueue");
    email_service::enqueue(db, message, "manual-dedupe".to_owned())
        .await
        .expect("segundo enqueue");

    let deduped = Outbox::find()
        .filter(OutboxColumn::DedupeKey.eq("manual-dedupe"))
        .count(db)
        .await
        .expect("dedupe");
    assert_eq!(deduped, 1);

    // --- falha reagenda com backoff e incrementa attempts ---
    let summary = email_service::process_pending(db, &FailingMailer, 10)
        .await
        .expect("processar com falha");
    assert_eq!(summary.claimed, 1);
    assert_eq!(summary.retried, 1);

    let manual = Outbox::find()
        .filter(OutboxColumn::DedupeKey.eq("manual-dedupe"))
        .one(db)
        .await
        .expect("buscar manual")
        .expect("manual existe");
    assert_eq!(manual.status, "pending");
    assert_eq!(manual.attempts, 1);
    assert!(manual.last_error.is_some());

    // O backoff segura a nova tentativa antes do horário agendado.
    let too_soon = email_service::process_pending(db, &RecordingMailer::default(), 10)
        .await
        .expect("reprocessar cedo demais");
    assert_eq!(too_soon.claimed, 0);

    // Vencido o prazo, a tentativa seguinte entrega.
    Outbox::update_many()
        .col_expr(OutboxColumn::NextAttemptAt, Expr::current_timestamp())
        .filter(OutboxColumn::DedupeKey.eq("manual-dedupe"))
        .exec(db)
        .await
        .expect("liberar a nova tentativa");

    let recording = RecordingMailer::default();
    let summary = email_service::process_pending(db, &recording, 10)
        .await
        .expect("reprocessar");
    assert_eq!(summary.sent, 1);
    assert_eq!(recording.subjects(), vec!["Teste".to_owned()]);
}
