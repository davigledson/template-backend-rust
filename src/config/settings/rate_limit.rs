//! Rate limit do login (por conta e por IP).

use std::time::Duration;

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct RateLimitSettings {
    /// Tentativas de login por conta dentro da janela.
    pub login_account_attempts: u32,
    /// Tentativas de login por IP dentro da janela.
    pub login_ip_attempts: u32,
    pub login_window_secs: u64,
    /// Pedidos de recuperação de senha por conta dentro da janela.
    pub password_reset_account_attempts: u32,
    /// Pedidos de recuperação de senha por IP dentro da janela.
    pub password_reset_ip_attempts: u32,
    /// Reenvios de confirmação de e-mail por conta dentro da janela.
    pub verification_account_attempts: u32,
    /// Reenvios de confirmação de e-mail por IP dentro da janela.
    pub verification_ip_attempts: u32,
}

impl RateLimitSettings {
    pub fn window(&self) -> Duration {
        Duration::from_secs(self.login_window_secs)
    }

    pub(super) fn collect_errors(&self, problems: &mut Vec<String>) {
        if self.login_account_attempts == 0 {
            problems.push("rate_limit.login_account_attempts deve ser maior que zero".to_owned());
        }
        if self.login_ip_attempts == 0 {
            problems.push("rate_limit.login_ip_attempts deve ser maior que zero".to_owned());
        }
        if self.login_window_secs == 0 {
            problems.push("rate_limit.login_window_secs deve ser maior que zero".to_owned());
        }
        if self.password_reset_account_attempts == 0 {
            problems.push(
                "rate_limit.password_reset_account_attempts deve ser maior que zero".to_owned(),
            );
        }
        if self.password_reset_ip_attempts == 0 {
            problems
                .push("rate_limit.password_reset_ip_attempts deve ser maior que zero".to_owned());
        }
        if self.verification_account_attempts == 0 {
            problems.push(
                "rate_limit.verification_account_attempts deve ser maior que zero".to_owned(),
            );
        }
        if self.verification_ip_attempts == 0 {
            problems.push("rate_limit.verification_ip_attempts deve ser maior que zero".to_owned());
        }
    }
}

impl Default for RateLimitSettings {
    fn default() -> Self {
        Self {
            login_account_attempts: 5,
            login_ip_attempts: 20,
            login_window_secs: 900,
            password_reset_account_attempts: 3,
            password_reset_ip_attempts: 10,
            verification_account_attempts: 3,
            verification_ip_attempts: 10,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn errors(settings: &RateLimitSettings) -> String {
        let mut problems = Vec::new();
        settings.collect_errors(&mut problems);
        problems.join("; ")
    }

    #[test]
    fn rejeita_limites_zero() {
        assert!(errors(&RateLimitSettings::default()).is_empty());

        let settings = RateLimitSettings {
            login_account_attempts: 0,
            login_ip_attempts: 0,
            login_window_secs: 0,
            password_reset_account_attempts: 0,
            password_reset_ip_attempts: 0,
            verification_account_attempts: 0,
            verification_ip_attempts: 0,
        };
        let message = errors(&settings);
        assert!(message.contains("login_account_attempts"), "{message}");
        assert!(message.contains("login_ip_attempts"), "{message}");
        assert!(message.contains("login_window_secs"), "{message}");
    }
}
