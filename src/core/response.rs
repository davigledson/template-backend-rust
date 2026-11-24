//! Respostas e paginação padronizadas.
//!
//! Listas devolvem `{ items, page, perPage, total }` (camelCase).

use serde::{Deserialize, Serialize};
use validator::Validate;

const DEFAULT_PAGE: u64 = 1;
const DEFAULT_PER_PAGE: u64 = 20;
const MAX_PER_PAGE: u64 = 100;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Page<T> {
    pub items: Vec<T>,
    pub page: u64,
    pub per_page: u64,
    pub total: u64,
}

impl<T> Page<T> {
    pub fn new(items: Vec<T>, page: u64, per_page: u64, total: u64) -> Self {
        Self {
            items,
            page,
            per_page,
            total,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct PageQuery {
    #[serde(default = "default_page")]
    #[validate(range(min = 1))]
    pub page: u64,
    #[serde(default = "default_per_page")]
    #[validate(range(min = 1, max = 100))]
    pub per_page: u64,
}

impl Default for PageQuery {
    fn default() -> Self {
        Self {
            page: DEFAULT_PAGE,
            per_page: DEFAULT_PER_PAGE,
        }
    }
}

impl PageQuery {
    /// Deslocamento para o banco, sempre calculado sobre um `per_page` seguro.
    pub fn offset(&self) -> u64 {
        self.page.saturating_sub(1) * self.per_page()
    }

    pub fn per_page(&self) -> u64 {
        self.per_page.clamp(1, MAX_PER_PAGE)
    }
}

const fn default_page() -> u64 {
    DEFAULT_PAGE
}

const fn default_per_page() -> u64 {
    DEFAULT_PER_PAGE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_usa_defaults() {
        let query = PageQuery::default();
        assert_eq!(query.page, 1);
        assert_eq!(query.per_page, 20);
        assert_eq!(query.offset(), 0);
    }

    #[test]
    fn per_page_e_limitado() {
        let query = PageQuery {
            page: 3,
            per_page: 10_000,
        };
        assert_eq!(query.per_page(), MAX_PER_PAGE);
        assert_eq!(query.offset(), 2 * MAX_PER_PAGE);
    }

    #[test]
    fn pagina_zero_nao_quebra_offset() {
        let query = PageQuery {
            page: 0,
            per_page: 20,
        };
        assert_eq!(query.offset(), 0);
    }
}
