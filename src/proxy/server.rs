use crate::mapping::types::MappingFile;
use crate::proxy::handler::{proxy_handler, AppState};
use axum::routing::any;
use axum::Router;
use std::net::SocketAddr;
use std::sync::Arc;
use url::Url;

pub async fn serve(
    mapping: MappingFile,
    target_url: Url,
    listen: SocketAddr,
    log_unmapped: bool,
) -> anyhow::Result<()> {
    let (mapped, total) = mapping.coverage();
    tracing::info!(
        mapped,
        total,
        target = %target_url,
        listen = %listen,
        "starting shotgun proxy"
    );

    let state = Arc::new(AppState {
        mapping,
        target_url,
        client: reqwest::Client::new(),
        log_unmapped,
    });

    let app = Router::new()
        .fallback(any(proxy_handler))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(listen).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
