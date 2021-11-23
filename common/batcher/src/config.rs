use time::Duration;

/// The parameters for the batching process.
#[derive(Debug, Clone)]
pub struct BatchConfig {
    event_jitter: Duration,
    delivery_jitter: Duration,
    message_leap_limit: Duration,
}

impl BatchConfig {
    /// Get the largest expected variation in event times.
    pub fn event_jitter(&self) -> Duration {
        self.event_jitter
    }

    /// Get the largest expected variation in delivery times.
    pub fn delivery_jitter(&self) -> Duration {
        self.delivery_jitter
    }

    /// Get the largest expected time discontinuity.
    pub fn message_leap_limit(&self) -> Duration {
        self.message_leap_limit
    }
}

/// Used to configure the parameters for batching. Start here.
#[derive(Debug, Default)]
pub struct BatchConfigBuilder {}

impl BatchConfigBuilder {
    /// Start configuring the batching parameters.
    pub fn new() -> BatchConfigBuilder {
        BatchConfigBuilder {}
    }

    /// Set the largest expected variation in event times, in milliseconds.
    pub fn event_jitter(self, event_jitter: u32) -> EventBatchConfigBuilder {
        EventBatchConfigBuilder { event_jitter }
    }
}

/// Used to configure the parameters for batching.
#[derive(Debug)]
pub struct EventBatchConfigBuilder {
    event_jitter: u32,
}

impl EventBatchConfigBuilder {
    /// Set the largest expected variation in delivery times, in milliseconds.
    pub fn delivery_jitter(self, delivery_jitter: u32) -> DeliveryBatchConfigBuilder {
        DeliveryBatchConfigBuilder {
            event_jitter: self.event_jitter,
            delivery_jitter,
        }
    }
}

/// Used to configure the parameters for batching.
#[derive(Debug)]
pub struct DeliveryBatchConfigBuilder {
    event_jitter: u32,
    delivery_jitter: u32,
}

impl DeliveryBatchConfigBuilder {
    /// Set the largest expected time discontinuity, in milliseconds.
    pub fn message_leap_limit(self, message_leap_limit: u32) -> BuildableBatchConfigBuilder {
        BuildableBatchConfigBuilder {
            event_jitter: self.event_jitter,
            delivery_jitter: self.delivery_jitter,
            message_leap_limit,
        }
    }
}

/// Used to configure the parameters for batching.
#[derive(Debug)]
pub struct BuildableBatchConfigBuilder {
    event_jitter: u32,
    delivery_jitter: u32,
    message_leap_limit: u32,
}

impl BuildableBatchConfigBuilder {
    /// Finalise the batching parameters.
    pub fn build(self) -> BatchConfig {
        let event_jitter = Duration::milliseconds(self.event_jitter as i64);
        let delivery_jitter = Duration::milliseconds(self.delivery_jitter as i64);
        let message_leap_limit = Duration::milliseconds(self.message_leap_limit as i64);

        BatchConfig {
            event_jitter,
            delivery_jitter,
            message_leap_limit,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_config() {
        let config = BatchConfigBuilder::new()
            .event_jitter(1)
            .delivery_jitter(2)
            .message_leap_limit(3)
            .build();

        assert_eq!(config.event_jitter(), Duration::milliseconds(1));
        assert_eq!(config.delivery_jitter(), Duration::milliseconds(2));
        assert_eq!(config.message_leap_limit(), Duration::milliseconds(3));
    }
}
