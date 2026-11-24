//! Papéis do sistema.
//!
//! Papel é um pacote de permissões. Handlers não devem comparar papéis:
//! use `AuthUser::require(Permission)`.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Admin,
    #[default]
    User,
}

impl Role {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Admin => "admin",
            Self::User => "user",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "admin" => Some(Self::Admin),
            "user" => Some(Self::User),
            _ => None,
        }
    }
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializa_em_minusculas() {
        let json = serde_json::to_string(&Role::Admin).unwrap_or_default();
        assert_eq!(json, "\"admin\"");
    }

    #[test]
    fn parse_aceita_apenas_papeis_conhecidos() {
        assert_eq!(Role::parse("admin"), Some(Role::Admin));
        assert_eq!(Role::parse("user"), Some(Role::User));
        assert_eq!(Role::parse("root"), None);
    }
}
