//! Política de segurança aplicada no login e nos cabeçalhos HTTP.

use serde::Deserialize;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct SecuritySettings {
    /// Exige e-mail confirmado para entrar. Precisa de e-mail transacional
    /// configurado, porque a confirmação chega por e-mail.
    pub require_email_verified: bool,
    /// `Strict-Transport-Security` em segundos. `0` desliga o cabeçalho.
    /// Só faz efeito quando a API é servida por HTTPS.
    pub hsts_max_age_secs: u64,
}
