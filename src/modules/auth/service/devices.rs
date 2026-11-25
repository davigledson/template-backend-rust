//! Sessões por dispositivo: listar e encerrar.

use uuid::Uuid;

use crate::{
    core::{auth::extractor::AuthUser, error::AppError, metrics, state::AppState},
    modules::{
        auth::{
            dto::{SessionList, SessionOut},
            repository,
        },
        users::service as users_service,
    },
};

/// Sessões ativas do usuário, da mais recente para a mais antiga.
pub async fn list(state: &AppState, auth: &AuthUser) -> Result<SessionList, AppError> {
    users_service::require_session(&state.db, auth).await?;

    let sessions = repository::list_sessions(&state.cache, auth.id).await?;
    let items = sessions
        .into_iter()
        .map(|session| SessionOut::from_record(session, auth.family_id))
        .collect();

    Ok(SessionList { items })
}

/// Encerra uma sessão. A sessão atual só sai pelo `logout`.
pub async fn revoke(state: &AppState, auth: &AuthUser, family_id: Uuid) -> Result<(), AppError> {
    users_service::require_session(&state.db, auth).await?;

    if family_id == auth.family_id {
        return Err(AppError::BadRequest(
            "use o logout para encerrar a sessão atual".to_owned(),
        ));
    }

    // Confere a posse antes de revogar: evita derrubar a sessão de outra conta.
    if !repository::remove_session(&state.cache, auth.id, family_id).await? {
        return Err(AppError::NotFound("sessão não encontrada".to_owned()));
    }

    repository::revoke_family(&state.cache, family_id, state.config.jwt.refresh_ttl()).await?;
    metrics::session_revoked();
    Ok(())
}

/// Remove os metadados de todas as sessões do usuário.
///
/// Best-effort: o `token_version` já invalidou os tokens, então uma falha aqui
/// só deixa a lista de dispositivos desatualizada até o TTL.
pub async fn clear_sessions(state: &AppState, user_id: Uuid) {
    if let Err(error) = repository::clear_sessions(&state.cache, user_id).await {
        tracing::warn!(error = %error, "falha ao limpar as sessões do usuário");
    }
}
