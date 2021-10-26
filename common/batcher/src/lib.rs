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
