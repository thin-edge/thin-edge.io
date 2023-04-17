//! Group together events that are close in time.

mod batch;
mod batchable;
mod batcher;
mod config;
mod driver;

pub use crate::batchable::Batchable;
pub use crate::batcher::Batcher;
pub use crate::config::BatchConfig;
pub use crate::config::BatchConfigBuilder;
pub use crate::config::BuildableBatchConfigBuilder;
pub use crate::config::DeliveryBatchConfigBuilder;
pub use crate::config::EventBatchConfigBuilder;
pub use crate::driver::BatchDriver;
pub use crate::driver::BatchDriverInput;
pub use crate::driver::BatchDriverOutput;
use std::convert::Infallible;
use tedge_actors::Builder;
use tedge_actors::DynSender;
use tedge_actors::MessageSink;
use tedge_actors::MessageSource;
use tedge_actors::NoConfig;
use tedge_actors::RuntimeRequest;
use tedge_actors::RuntimeRequestSink;
use tedge_actors::ServiceProvider;
use tedge_actors::SimpleMessageBoxBuilder;

pub struct BatchingActorBuilder<B: Batchable> {
    batching_window: u32,
    maximum_message_delay: u32,
    message_leap_limit: u32,
    message_box: SimpleMessageBoxBuilder<BatchDriverInput<B>, BatchDriverOutput<B>>,
}

impl<B: Batchable> Default for BatchingActorBuilder<B> {
    fn default() -> Self {
        BatchingActorBuilder {
            batching_window: 500,
            maximum_message_delay: 400, // Heuristic delay that should work out well on an Rpi
            message_leap_limit: 0,
            message_box: SimpleMessageBoxBuilder::new("Event batcher", 16),
        }
    }
}

impl<B: Batchable> BatchingActorBuilder<B> {
    pub fn with_batching_window(self, batching_window: u32) -> Self {
        Self {
            batching_window,
            ..self
        }
    }

    pub fn with_maximum_message_delay(self, maximum_message_delay: u32) -> Self {
        Self {
            maximum_message_delay,
            ..self
        }
    }

    pub fn with_message_leap_limit(self, message_leap_limit: u32) -> Self {
        Self {
            message_leap_limit,
            ..self
        }
    }
}

// FIXME: This implementation highlights something new to me.
//        For some actor it makes little sense to impl ServiceProvider,
//        as the consumer of the outputs is likely a *different* actor as the producer of the inputs
impl<B: Batchable> ServiceProvider<BatchDriverInput<B>, BatchDriverOutput<B>, NoConfig>
    for BatchingActorBuilder<B>
{
    fn connect_consumer(
        &mut self,
        config: NoConfig,
        response_sender: DynSender<BatchDriverOutput<B>>,
    ) -> DynSender<BatchDriverInput<B>> {
        self.message_box.connect_consumer(config, response_sender)
    }
}

impl<B: Batchable> MessageSink<BatchDriverInput<B>, NoConfig> for BatchingActorBuilder<B> {
    fn get_config(&self) -> NoConfig {
        NoConfig
    }
    fn get_sender(&self) -> DynSender<BatchDriverInput<B>> {
        self.message_box.get_sender()
    }
}

impl<B: Batchable> MessageSource<BatchDriverOutput<B>, NoConfig> for BatchingActorBuilder<B> {
    fn register_peer(&mut self, config: NoConfig, sender: DynSender<BatchDriverOutput<B>>) {
        self.message_box.register_peer(config, sender)
    }
}

impl<B: Batchable> RuntimeRequestSink for BatchingActorBuilder<B> {
    fn get_signal_sender(&self) -> DynSender<RuntimeRequest> {
        self.message_box.get_signal_sender()
    }
}

impl<B: Batchable> Builder<BatchDriver<B>> for BatchingActorBuilder<B> {
    type Error = Infallible;

    fn try_build(self) -> Result<BatchDriver<B>, Self::Error> {
        Ok(self.build())
    }

    fn build(self) -> BatchDriver<B> {
        let batch_config = BatchConfigBuilder::new()
            .event_jitter(self.batching_window)
            .delivery_jitter(self.maximum_message_delay)
            .message_leap_limit(self.message_leap_limit)
            .build();
        let batcher = Batcher::new(batch_config);
        let message_box = self.message_box.build();
        BatchDriver::new(batcher, message_box)
    }
}
