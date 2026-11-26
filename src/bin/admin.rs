//! CLI de manutenção: bootstrap de admin e operação do outbox de e-mail.
//!
//! Nunca roda no boot da API. Exemplos:
//!   cargo run --bin admin -- create --email admin@empresa.com --name "Admin"
//!   ADMIN_PASSWORD=... cargo run --bin admin -- create --email ... --name ...
//!   cargo run --bin admin -- outbox list --status failed
//!   cargo run --bin admin -- outbox retry <id>

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::process::ExitCode;

use clap::{Parser, Subcommand};
use sea_orm::DatabaseConnection;
use template_backend_rust::{
    config::Settings,
    core::auth::{password::MIN_PASSWORD_LEN, role::Role},
    core::cache::Cache,
    core::mail::mask_email,
    database::connection,
    modules::{
        audit::service::Actor,
        auth::service as auth_service,
        email::service as email_service,
        users::service::{self, NewUser},
    },
};
use uuid::Uuid;

#[derive(Parser)]
#[command(name = "admin", about = "Bootstrap e manutenção de usuários")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Cria um administrador (o primeiro exige banco sem admins ativos).
    Create {
        #[arg(long)]
        email: String,
        #[arg(long)]
        name: String,
        /// Permite criar outro admin mesmo já existindo um.
        #[arg(long)]
        force: bool,
    },
    /// Promove um usuário existente a administrador.
    Promote {
        #[arg(long)]
        email: String,
    },
    /// Redefine a senha de um usuário (invalida as sessões dele).
    ResetPassword {
        #[arg(long)]
        email: String,
    },
    /// Confirma o e-mail de um usuário ou de todos os pendentes.
    VerifyEmail {
        /// E-mail do usuário; omita com --all para o backfill.
        #[arg(long, conflicts_with = "all")]
        email: Option<String>,
        /// Confirma todos os usuários com e-mail pendente.
        #[arg(long)]
        all: bool,
    },
    /// Inspeciona e reprocessa o outbox de e-mail.
    Outbox {
        #[command(subcommand)]
        action: OutboxCommand,
    },
}

#[derive(Subcommand)]
enum OutboxCommand {
    /// Lista itens de um status (`pending`, `failed` ou `sent`).
    List {
        #[arg(long, default_value = "failed")]
        status: String,
        #[arg(long, default_value_t = 20)]
        limit: u64,
    },
    /// Devolve um item `pending`/`failed` ao processamento imediato.
    Retry { id: Uuid },
    /// Reprocessa todos os itens de um status (`pending` ou `failed`).
    RetryAll {
        #[arg(long, default_value = "failed")]
        status: String,
        /// Zera o contador de tentativas antes de reagendar.
        #[arg(long)]
        force: bool,
    },
}

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("erro: {error}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let settings = Settings::load_unvalidated()?;
    let db = connection::connect(&settings.database).await?;
    let cache = Cache::connect(&settings.redis)?;

    match cli.command {
        Command::Create { email, name, force } => {
            create_admin(&db, &cache, &email, &name, force).await?
        }
        Command::Promote { email } => promote(&db, &email).await?,
        Command::ResetPassword { email } => reset_password(&db, &cache, &email).await?,
        Command::VerifyEmail { email, all } => verify_email(&db, email.as_deref(), all).await?,
        Command::Outbox { action } => outbox(&db, action).await?,
    }

    Ok(())
}

async fn create_admin(
    db: &DatabaseConnection,
    cache: &Cache,
    email: &str,
    name: &str,
    force: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if !force && service::count_active_admins(db).await? > 0 {
        return Err("já existe administrador ativo; use --force para criar outro".into());
    }
    if service::find_by_email(db, email).await?.is_some() {
        return Err("e-mail já cadastrado; use promote ou reset-password".into());
    }

    let password = read_password(true)?;
    let user = service::create(
        db,
        cache,
        Actor::system(),
        NewUser {
            name: name.to_owned(),
            email: email.to_owned(),
            password,
            role: Role::Admin,
        },
    )
    .await?;

    println!("admin criado: {} ({})", user.email, user.id);
    Ok(())
}

async fn promote(db: &DatabaseConnection, email: &str) -> Result<(), Box<dyn std::error::Error>> {
    let user = service::find_by_email(db, email)
        .await?
        .ok_or("usuário não encontrado")?;

    if user.role == Role::Admin.as_str() {
        println!("{} já é administrador", user.email);
        return Ok(());
    }

    service::set_role(db, Actor::system(), user.id, Role::Admin).await?;
    println!("usuário promovido a admin: {}", user.email);
    Ok(())
}

async fn reset_password(
    db: &DatabaseConnection,
    cache: &Cache,
    email: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let user = service::find_by_email(db, email)
        .await?
        .ok_or("usuário não encontrado")?;

    let password = read_password(true)?;
    service::set_password(db, Actor::system(), user.id, &password).await?;
    // O `token_version` já invalida os tokens; isto limpa a lista de dispositivos.
    auth_service::clear_sessions(cache, user.id).await;
    println!("senha redefinida para {}", user.email);
    Ok(())
}

async fn verify_email(
    db: &DatabaseConnection,
    email: Option<&str>,
    all: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if all {
        let total = service::verify_all_emails(db, Actor::system()).await?;
        println!("e-mails confirmados: {total}");
        return Ok(());
    }

    let email = email.ok_or("informe --email ou --all")?;
    let user = service::find_by_email(db, email)
        .await?
        .ok_or("usuário não encontrado")?;

    if service::verify_email(db, Actor::system(), user.id).await? {
        println!("e-mail confirmado: {}", user.email);
    } else {
        println!("e-mail já estava confirmado: {}", user.email);
    }

    Ok(())
}

async fn outbox(
    db: &DatabaseConnection,
    action: OutboxCommand,
) -> Result<(), Box<dyn std::error::Error>> {
    match action {
        OutboxCommand::List { status, limit } => {
            let limit = limit.clamp(1, 200);
            let items = email_service::list(db, &status, limit).await?;

            if items.is_empty() {
                println!("nenhum e-mail com status {status}");
                return Ok(());
            }

            println!(
                "{:<38} {:<10} {:>3} {:<30} assunto",
                "id", "status", "tent", "destinatário"
            );
            for item in items {
                println!(
                    "{:<38} {:<10} {:>3} {:<30} {}",
                    item.id,
                    item.status,
                    item.attempts,
                    mask_email(&item.to_email),
                    item.subject
                );
            }
        }
        OutboxCommand::Retry { id } => {
            if email_service::retry(db, id).await? {
                println!("reagendado: {id}");
            } else {
                return Err("item não encontrado ou já enviado".into());
            }
        }
        OutboxCommand::RetryAll { status, force } => {
            if !matches!(status.as_str(), "pending" | "failed") {
                return Err("status aceito: pending ou failed".into());
            }

            let total = email_service::retry_all(db, &status, force).await?;
            println!("reagendados: {total} item(ns) de status {status}");
        }
    }

    Ok(())
}

fn read_password(confirmation: bool) -> Result<String, Box<dyn std::error::Error>> {
    if let Ok(password) = std::env::var("ADMIN_PASSWORD") {
        return Ok(password);
    }

    let password =
        rpassword::prompt_password(format!("Senha (mín. {MIN_PASSWORD_LEN} caracteres): "))?;

    if confirmation {
        let again = rpassword::prompt_password("Confirme a senha: ")?;
        if password != again {
            return Err("as senhas não conferem".into());
        }
    }

    Ok(password)
}
