//! Encerramento gracioso: Ctrl+C ou SIGTERM.

use std::future::pending;

/// Resolve quando o processo recebe Ctrl+C ou SIGTERM.
pub async fn signal() {
    let ctrl_c = async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            tracing::error!(error = %error, "falha ao escutar Ctrl+C");
            pending::<()>().await;
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(error) => {
                tracing::error!(error = %error, "falha ao escutar SIGTERM");
                pending::<()>().await;
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = pending::<()>();

    tokio::select! {
        () = ctrl_c => tracing::info!("sinal de encerramento recebido (Ctrl+C)"),
        () = terminate => tracing::info!("sinal de encerramento recebido (SIGTERM)"),
    }
}
