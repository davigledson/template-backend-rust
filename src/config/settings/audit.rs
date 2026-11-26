//! Retenção da auditoria.
//!
//! A tabela é append-only para a aplicação; o worker remove registros antigos
//! conforme a política abaixo. `0` mantém tudo para sempre.

use serde::Deserialize;

pub const DEFAULT_RETENTION_DAYS: u64 = 365;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct AuditSettings {
    /// Dias de retenção do `audit_log`; `0` desliga o expurgo.
    pub retention_days: u64,
}

impl Default for AuditSettings {
    fn default() -> Self {
        Self {
            retention_days: DEFAULT_RETENTION_DAYS,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn padrao_mantem_um_ano() {
        assert_eq!(
            AuditSettings::default().retention_days,
            DEFAULT_RETENTION_DAYS
        );
    }
}
