//! Filtros e paginação da listagem.

use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};
use validator::Validate;

use crate::core::auth::role::Role;

const DEFAULT_PAGE: u64 = 1;
const DEFAULT_PER_PAGE: u64 = 20;
const MAX_PER_PAGE: u64 = 100;

fn default_page() -> u64 {
    DEFAULT_PAGE
}

fn default_per_page() -> u64 {
    DEFAULT_PER_PAGE
}

/// Filtros da listagem. `active` ausente devolve ativos e inativos.
#[derive(Debug, Deserialize, Validate, ToSchema, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct ListUsersQuery {
    #[serde(default = "default_page")]
    #[validate(range(min = 1, max = 1_000_000))]
    #[param(minimum = 1, maximum = 1000000, default = 1)]
    pub page: u64,
    #[serde(default = "default_per_page")]
    #[validate(range(min = 1, max = 100))]
    #[param(minimum = 1, maximum = 100, default = 20)]
    pub per_page: u64,
    /// Busca por nome ou e-mail (case-insensitive).
    #[validate(length(max = 120))]
    pub search: Option<String>,
    pub role: Option<Role>,
    pub active: Option<bool>,
}

impl ListUsersQuery {
    pub fn page(&self) -> u64 {
        self.page.max(1)
    }

    pub fn per_page(&self) -> u64 {
        self.per_page.clamp(1, MAX_PER_PAGE)
    }

    pub fn offset(&self) -> u64 {
        self.page()
            .saturating_sub(1)
            .saturating_mul(self.per_page())
            .min(i64::MAX as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query(page: u64, per_page: u64) -> ListUsersQuery {
        ListUsersQuery {
            page,
            per_page,
            search: None,
            role: None,
            active: None,
        }
    }

    #[test]
    fn paginacao_nunca_estoura_limites() {
        let q = query(0, 0);
        assert_eq!(q.page(), 1);
        assert_eq!(q.per_page(), 1);
        assert_eq!(q.offset(), 0);

        let q = query(2, 500);
        assert_eq!(q.page(), 2);
        assert_eq!(q.per_page(), 100);
        assert_eq!(q.offset(), 100);
    }

    #[test]
    fn offset_maximo_nao_estoura() {
        let q = query(u64::MAX, 100);
        assert_eq!(q.offset(), i64::MAX as u64);
    }
}
