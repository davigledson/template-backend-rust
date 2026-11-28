//! Hash e verificação de senha com Argon2id.
//!
//! O mesmo código é usado pelo login e pelo CLI de bootstrap.

use argon2::{
    Argon2, PasswordHasher, PasswordVerifier,
    password_hash::{self, phc::PasswordHash},
};
use thiserror::Error;
use tokio::sync::{Semaphore, SemaphorePermit};

pub const MIN_PASSWORD_LEN: usize = 12;

/// Quantos hashes/verificações Argon2id podem rodar ao mesmo tempo.
///
/// O `Argon2::default()` usa ~19 MiB por tarefa. Com 2 vCPUs e `mem_limit` de
/// 384 MiB no API, 2 tarefas (≈38 MiB) deixam margem para o resto do processo;
/// aumentar só com memória e CPU medidas. Sem este limite, o pool de
/// `spawn_blocking` pode abrir dezenas de hashes e derrubar o container.
const MAX_CONCURRENT_PASSWORD_TASKS: usize = 2;

static PASSWORD_TASK_SLOTS: Semaphore = Semaphore::const_new(MAX_CONCURRENT_PASSWORD_TASKS);

/// Hash válido usado para igualar o custo quando o e-mail não existe ou está
/// inativo, evitando enumeração por tempo de resposta.
///
/// Parâmetros iguais aos do `Argon2::default()` (Argon2id, 19 MiB, 2 iterações).
pub const DUMMY_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$ZHVtbXktc2FsdC0xMjM$FfsNHH4ec0g/fvy+PLyimq8b4y2SEGvoTwo5s4p6ipA";

#[derive(Debug, Error)]
pub enum PasswordError {
    #[error("falha ao gerar salt: {0}")]
    Salt(String),
    #[error("senha inválida: {0}")]
    Policy(String),
    #[error("hash de senha inválido: {0}")]
    InvalidHash(#[from] password_hash::phc::Error),
    #[error("falha ao processar a senha: {0}")]
    Hash(#[from] password_hash::Error),
    #[error("falha ao executar a tarefa de senha: {0}")]
    Task(String),
}

pub fn hash(password: &str) -> Result<String, PasswordError> {
    let salt = password_hash::try_generate_salt()
        .map_err(|error| PasswordError::Salt(error.to_string()))?;
    let hash = Argon2::default().hash_password_with_salt(password.as_bytes(), &salt)?;
    Ok(hash.to_string())
}

pub fn verify(password: &str, encoded: &str) -> Result<bool, PasswordError> {
    let parsed = PasswordHash::new(encoded)?;
    match Argon2::default().verify_password(password.as_bytes(), &parsed) {
        Ok(()) => Ok(true),
        Err(password_hash::Error::PasswordInvalid) => Ok(false),
        Err(error) => Err(PasswordError::Hash(error)),
    }
}

/// Reserva uma vaga antes de acionar o pool de blocking. A vaga é liberada
/// quando a tarefa termina (inclusive em erro).
async fn acquire_task_slot() -> Result<SemaphorePermit<'static>, PasswordError> {
    PASSWORD_TASK_SLOTS
        .acquire()
        .await
        .map_err(|error| PasswordError::Task(error.to_string()))
}

/// Igual ao [`hash`], mas fora do thread do runtime Tokio e com concorrência
/// limitada.
pub async fn hash_async(password: String) -> Result<String, PasswordError> {
    let _permit = acquire_task_slot().await?;

    tokio::task::spawn_blocking(move || hash(&password))
        .await
        .map_err(|error| PasswordError::Task(error.to_string()))?
}

/// Igual ao [`verify`], mas fora do thread do runtime Tokio e com concorrência
/// limitada.
pub async fn verify_async(password: String, encoded: String) -> Result<bool, PasswordError> {
    let _permit = acquire_task_slot().await?;

    tokio::task::spawn_blocking(move || verify(&password, &encoded))
        .await
        .map_err(|error| PasswordError::Task(error.to_string()))?
}

/// Política mínima; o restante (zxcvbn, senhas comuns) fica para o projeto.
pub fn validate_strength(password: &str) -> Result<(), PasswordError> {
    if password.chars().count() < MIN_PASSWORD_LEN {
        return Err(PasswordError::Policy(format!(
            "a senha precisa ter ao menos {MIN_PASSWORD_LEN} caracteres"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_e_verificacao_funcionam() {
        let encoded = hash("uma-senha-bem-longa-123").expect("hash");
        assert!(verify("uma-senha-bem-longa-123", &encoded).expect("verify"));
        assert!(!verify("senha-errada", &encoded).expect("verify"));
    }

    #[test]
    fn hashes_diferentes_para_a_mesma_senha() {
        let a = hash("uma-senha-bem-longa-123").expect("hash");
        let b = hash("uma-senha-bem-longa-123").expect("hash");
        assert_ne!(a, b);
    }

    #[test]
    fn rejeita_senha_curta() {
        assert!(validate_strength("curta").is_err());
        assert!(validate_strength("uma-senha-bem-longa-123").is_ok());
    }

    #[tokio::test]
    async fn hash_e_verificacao_saem_do_thread_do_runtime() {
        let encoded = hash_async("uma-senha-bem-longa-123".to_owned())
            .await
            .expect("hash");
        assert!(
            verify_async("uma-senha-bem-longa-123".to_owned(), encoded.clone())
                .await
                .expect("verify")
        );
        assert!(
            !verify_async("senha-errada".to_owned(), encoded)
                .await
                .expect("verify")
        );
    }

    #[tokio::test]
    async fn erro_de_hash_nao_vaza_vaga() {
        // Se um erro não liberasse o semáforo, a terceira chamada ficaria
        // bloqueada esperando uma vaga que nunca volta.
        for _ in 0..(MAX_CONCURRENT_PASSWORD_TASKS * 3) {
            let result = verify_async("senha".to_owned(), "hash-invalido".to_owned()).await;
            assert!(result.is_err());
        }
    }

    #[test]
    fn hash_dummy_e_valido() {
        assert!(verify("dummy-password", DUMMY_HASH).expect("verify"));
        assert!(!verify("outra-senha", DUMMY_HASH).expect("verify"));
    }
}
