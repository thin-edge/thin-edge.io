use super::file_transfer::file_transfer_router;
use crate::file_transfer_server::error::FileTransferError;
use axum::Router;
use camino::Utf8PathBuf;
use futures::future::FutureExt;
use rustls::ServerConfig;
use std::future::Future;
use std::sync::Arc;
use std::sync::Mutex;
use tedge_api::EntityStore;
use tokio::io;
use tokio::net::TcpListener;

pub(crate) struct AgentState {
    file_transfer_dir: Utf8PathBuf,
    entity_store: Arc<Mutex<EntityStore>>,
}

impl AgentState {
    pub fn new(file_transfer_dir: Utf8PathBuf, entity_store: EntityStore) -> Self {
        AgentState {
            file_transfer_dir,
            entity_store: Arc::new(Mutex::new(entity_store)),
        }
    }
}

pub(crate) fn http_file_transfer_server(
    listener: TcpListener,
    rustls_config: Option<ServerConfig>,
    agent_state: AgentState,
) -> Result<impl Future<Output = io::Result<()>>, FileTransferError> {
    let router = router(agent_state);

    let listener = listener.into_std()?;

    let server = if let Some(rustls_config) = rustls_config {
        axum_tls::start_tls_server(listener, rustls_config, router).boxed()
    } else {
        axum_server::from_tcp(listener)
            .serve(router.into_make_service())
            .boxed()
    };

    Ok(server)
}

fn router(state: AgentState) -> Router {
    let entity_store_router = entity_store_router(state.entity_store);
    let file_transfer_router = file_transfer_router(state.file_transfer_dir.clone());

    Router::new()
        .nest("/tedge/entity-store", entity_store_router)
        .merge(file_transfer_router)
}
