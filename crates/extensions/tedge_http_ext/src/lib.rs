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

#[derive(Debug)]
pub struct HttpActor {
    config: ServerConfig,
    tls_client_config: rustls::ClientConfig,
}

impl HttpActor {
    pub fn new(tls_client_config: rustls::ClientConfig) -> Self {
        Self {
            config: <_>::default(),
            tls_client_config,
        }
    }

    pub fn builder(&self) -> ServerActorBuilder<HttpService, Concurrent> {
        ServerActorBuilder::new(
            HttpService::new(self.tls_client_config.clone()),
            &self.config,
            Concurrent,
        )
    }

    pub fn with_capacity(self, capacity: usize) -> Self {
        Self {
            config: self.config.with_capacity(capacity),
            ..self
        }
    }

    pub fn with_max_concurrency(self, max_concurrency: usize) -> Self {
        Self {
            config: self.config.with_max_concurrency(max_concurrency),
            ..self
        }
    }
}
