//! Hash e verificação de senha com Argon2id.
//!
//! O mesmo código é usado pelo login e pelo CLI de bootstrap.

use argon2::{
    Argon2, PasswordHasher, PasswordVerifier,
    password_hash::{self, phc::PasswordHash},
};
use thiserror::Error;

pub const MIN_PASSWORD_LEN: usize = 12;

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
}
