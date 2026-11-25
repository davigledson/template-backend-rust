//! Catálogo de permissões e o mapa papel → permissões.
//!
//! A política fica no código (versionada e testada); o banco guarda apenas
//! qual papel o usuário tem.

use crate::core::auth::role::Role;

/// Permissão nomeada no formato `recurso:acao`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Permission(&'static str);

impl Permission {
    pub const fn new(name: &'static str) -> Self {
        Self(name)
    }

    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

pub const USERS_READ: Permission = Permission::new("users:read");
pub const USERS_WRITE: Permission = Permission::new("users:write");
pub const USERS_DELETE: Permission = Permission::new("users:delete");
pub const PROFILE_WRITE: Permission = Permission::new("profile:write");
pub const AUDIT_READ: Permission = Permission::new("audit:read");

const ADMIN_PERMISSIONS: &[Permission] = &[
    USERS_READ,
    USERS_WRITE,
    USERS_DELETE,
    PROFILE_WRITE,
    AUDIT_READ,
];
const USER_PERMISSIONS: &[Permission] = &[PROFILE_WRITE];

pub fn permissions_for(role: Role) -> &'static [Permission] {
    match role {
        Role::Admin => ADMIN_PERMISSIONS,
        Role::User => USER_PERMISSIONS,
    }
}

impl Role {
    pub fn can(self, permission: Permission) -> bool {
        permissions_for(self).contains(&permission)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admin_pode_gerenciar_usuarios() {
        assert!(Role::Admin.can(USERS_READ));
        assert!(Role::Admin.can(USERS_WRITE));
        assert!(Role::Admin.can(USERS_DELETE));
        assert!(Role::Admin.can(AUDIT_READ));
    }

    #[test]
    fn usuario_comum_nao_gerencia_usuarios() {
        assert!(Role::User.can(PROFILE_WRITE));
        assert!(!Role::User.can(USERS_READ));
        assert!(!Role::User.can(USERS_DELETE));
        assert!(!Role::User.can(AUDIT_READ));
    }
}
