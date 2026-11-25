//! Política de segurança aplicada no login.

use serde::Deserialize;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct SecuritySettings {
    /// Exige e-mail confirmado para entrar. Precisa de e-mail transacional
    /// configurado, porque a confirmação chega por e-mail.
    pub require_email_verified: bool,
}
