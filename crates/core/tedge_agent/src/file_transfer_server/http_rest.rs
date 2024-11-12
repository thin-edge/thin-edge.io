use super::entity_store::entity_store_router;
use super::file_transfer::file_transfer_router;
use crate::file_transfer_server::error::FileTransferError;
use axum::Router;
use camino::Utf8PathBuf;
use futures::future::FutureExt;
use rustls::ServerConfig;
use std::future::Future;
use std::sync::Arc;
use std::sync::Mutex;
use tedge_actors::LoggingSender;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::EntityStore;
use tedge_mqtt_ext::MqttMessage;
use tokio::io;
use tokio::net::TcpListener;

#[derive(Clone)]
pub(crate) struct AgentState {
    pub(crate) file_transfer_dir: Utf8PathBuf,
    pub(crate) entity_store: Arc<Mutex<EntityStore>>,
    pub(crate) mqtt_schema: MqttSchema,
    pub(crate) mqtt_publisher: LoggingSender<MqttMessage>,
}

impl AgentState {
    pub fn new(
        file_transfer_dir: Utf8PathBuf,
        entity_store: Arc<Mutex<EntityStore>>,
        mqtt_schema: MqttSchema,
        mqtt_publisher: LoggingSender<MqttMessage>,
    ) -> Self {
        AgentState {
            file_transfer_dir,
            entity_store,
            mqtt_schema,
            mqtt_publisher,
        }
    }
}

pub(crate) fn http_server(
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
    let file_transfer_router = file_transfer_router(state.file_transfer_dir.clone());
    let entity_store_router = entity_store_router(state);

    Router::new()
        .nest("/tedge/entity-store", entity_store_router)
        .merge(file_transfer_router)
}
