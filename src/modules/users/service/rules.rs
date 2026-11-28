//! Validações puras compartilhadas pelas regras de `users`.

use sea_orm::{DbErr, SqlErr};
use uuid::Uuid;

use crate::core::{auth::role::Role, error::AppError};

/// E-mail sempre comparado em minúsculas e sem espaços nas pontas.
pub fn normalize_email(email: &str) -> String {
    email.trim().to_lowercase()
}

pub fn is_valid_email(email: &str) -> bool {
    let (local, domain) = match email.split_once('@') {
        Some(parts) => parts,
        None => return false,
    };

    !local.is_empty()
        && !domain.is_empty()
        && domain.contains('.')
        && !email.chars().any(char::is_whitespace)
        && email.len() <= 254
}

pub fn parse_role(value: &str) -> Result<Role, AppError> {
    Role::parse(value)
        .ok_or_else(|| AppError::internal(format!("papel inválido no banco: {value}")))
}

pub fn is_unique_violation(error: &DbErr) -> bool {
    matches!(error.sql_err(), Some(SqlErr::UniqueConstraintViolation(_)))
}

/// O alvo deixa de ser um administrador ativo.
pub fn loses_admin(current_role: Role, new_role: Role, new_active: bool) -> bool {
    current_role == Role::Admin && (new_role != Role::Admin || !new_active)
}

/// Impede auto-rebaixamento/autoexclusão e a remoção do último admin ativo.
///
/// `actor_id` é `None` quando a ação veio da CLI: sem autor HTTP, não há
/// auto-alteração a proteger.
pub fn ensure_change_allowed(
    actor_id: Option<Uuid>,
    target_id: Uuid,
    current_role: Role,
    new_active: bool,
    new_role: Role,
    active_admins: u64,
) -> Result<(), AppError> {
    if actor_id == Some(target_id) {
        if !new_active {
            return Err(AppError::Conflict(
                "não é possível desativar o próprio usuário".to_owned(),
            ));
        }
        if new_role != current_role {
            return Err(AppError::Conflict(
                "não é possível alterar o próprio papel".to_owned(),
            ));
        }
    }

    if loses_admin(current_role, new_role, new_active) && active_admins <= 1 {
        return Err(AppError::Conflict(
            "não é possível remover o último administrador ativo".to_owned(),
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normaliza_email() {
        assert_eq!(normalize_email("  A@B.COM "), "a@b.com");
    }

    #[test]
    fn valida_email_basico() {
        assert!(is_valid_email("a@b.com"));
        assert!(!is_valid_email("a@b"));
        assert!(!is_valid_email("@b.com"));
        assert!(!is_valid_email("a@"));
        assert!(!is_valid_email("a b@c.com"));
    }

    #[test]
    fn nao_permite_auto_desativacao() {
        let id = Uuid::now_v7();
        let error = ensure_change_allowed(Some(id), id, Role::Admin, false, Role::Admin, 2);
        assert!(matches!(error, Err(AppError::Conflict(_))));
    }

    #[test]
    fn nao_permite_auto_rebaixamento() {
        let id = Uuid::now_v7();
        let error = ensure_change_allowed(Some(id), id, Role::Admin, true, Role::User, 2);
        assert!(matches!(error, Err(AppError::Conflict(_))));
    }

    #[test]
    fn nao_permite_remover_ultimo_admin() {
        let actor = Uuid::now_v7();
        let target = Uuid::now_v7();

        let demote = ensure_change_allowed(Some(actor), target, Role::Admin, true, Role::User, 1);
        assert!(matches!(demote, Err(AppError::Conflict(_))));

        let deactivate =
            ensure_change_allowed(Some(actor), target, Role::Admin, false, Role::Admin, 1);
        assert!(matches!(deactivate, Err(AppError::Conflict(_))));
    }

    #[test]
    fn permite_alterar_quando_ha_outro_admin() {
        let actor = Uuid::now_v7();
        let target = Uuid::now_v7();

        assert!(
            ensure_change_allowed(Some(actor), target, Role::Admin, true, Role::User, 2).is_ok()
        );
        assert!(
            ensure_change_allowed(Some(actor), target, Role::Admin, false, Role::Admin, 2).is_ok()
        );
        assert!(
            ensure_change_allowed(Some(actor), target, Role::User, false, Role::User, 1).is_ok()
        );
    }

    #[test]
    fn admin_pode_editar_a_si_mesmo_sem_mudar_papel() {
        let id = Uuid::now_v7();
        assert!(ensure_change_allowed(Some(id), id, Role::Admin, true, Role::Admin, 1).is_ok());
    }
}
