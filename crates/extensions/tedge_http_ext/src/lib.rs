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

#[derive(Debug, Default)]
pub struct HttpActor {
    config: ServerConfig,
}

impl HttpActor {
    pub fn new() -> Self {
        HttpActor::default()
    }

    pub fn builder(&self) -> ServerActorBuilder<HttpService, Concurrent> {
        ServerActorBuilder::new(HttpService::new(), &self.config, Concurrent)
    }

    pub fn with_capacity(self, capacity: usize) -> Self {
        Self {
            config: self.config.with_capacity(capacity),
        }
    }

    pub fn with_max_concurrency(self, max_concurrency: usize) -> Self {
        Self {
            config: self.config.with_max_concurrency(max_concurrency),
        }
    }
}
