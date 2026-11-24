//! CLI de bootstrap: cria o primeiro admin, promove usuários e redefine senha.
//!
//! Nunca roda no boot da API. Exemplos:
//!   cargo run --bin admin -- create --email admin@empresa.com --name "Admin"
//!   ADMIN_PASSWORD=... cargo run --bin admin -- create --email ... --name ...

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::process::ExitCode;

use clap::{Parser, Subcommand};
use sea_orm::DatabaseConnection;
use template_backend_rust::{
    config::Settings,
    core::auth::{password::MIN_PASSWORD_LEN, role::Role},
    database::connection,
    modules::users::service::{self, NewUser},
};

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

    match cli.command {
        Command::Create { email, name, force } => create_admin(&db, &email, &name, force).await?,
        Command::Promote { email } => promote(&db, &email).await?,
        Command::ResetPassword { email } => reset_password(&db, &email).await?,
    }

    Ok(())
}

async fn create_admin(
    db: &DatabaseConnection,
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

    service::set_role(db, user.id, Role::Admin).await?;
    println!("usuário promovido a admin: {}", user.email);
    Ok(())
}

async fn reset_password(
    db: &DatabaseConnection,
    email: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let user = service::find_by_email(db, email)
        .await?
        .ok_or("usuário não encontrado")?;

    let password = read_password(true)?;
    service::set_password(db, user.id, &password).await?;
    println!("senha redefinida para {}", user.email);
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
