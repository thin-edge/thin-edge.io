mod actor;
mod messages;

#[cfg(test)]
mod tests;

#[cfg(feature = "test_helpers")]
pub mod test_helpers;

pub use messages::*;

use actor::*;
use tedge_actors::Concurrent;
use tedge_actors::ServerActorBuilder;
use tedge_actors::ServerConfig;
use tedge_config::TEdgeConfig;

#[derive(Debug)]
pub struct HttpActor {
    config: ServerConfig,
    tls_client_config: rustls::ClientConfig,
}

impl HttpActor {
    pub fn new(tedge_config: &TEdgeConfig) -> Self {
        Self {
            config: <_>::default(),
            tls_client_config: tedge_config.cloud_client_tls_config(),
        }
    }

    pub fn builder(&self) -> ServerActorBuilder<HttpService, Concurrent> {
        ServerActorBuilder::new(
            HttpService::new(self.tls_client_config.clone()),
            &self.config,
            Concurrent,
        )
    }

    pub fn with_capacity(self, capacity: usize, tedge_config: &TEdgeConfig) -> Self {
        Self {
            config: self.config.with_capacity(capacity),
            ..Self::new(tedge_config)
        }
    }

    pub fn with_max_concurrency(self, max_concurrency: usize, tedge_config: &TEdgeConfig) -> Self {
        Self {
            config: self.config.with_max_concurrency(max_concurrency),
            ..Self::new(tedge_config)
        }
    }
}
