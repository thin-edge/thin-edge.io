use crate::batchable::Batchable;
use crate::batcher::Batcher;
use crate::batcher::BatcherOutput;
use async_trait::async_trait;
use std::collections::BTreeSet;
use std::time::Duration;
use tedge_actors::Actor;
use tedge_actors::ChannelError;
use tedge_actors::MessageReceiver;
use tedge_actors::RuntimeError;
use tedge_actors::Sender;
use tedge_actors::SimpleMessageBox;
use time::OffsetDateTime;

/// Input message to the BatchDriver's input channel.
#[derive(Debug)]
pub enum BatchDriverInput<B: Batchable> {
    /// Message representing a new item to batch.
    Event(B),
    /// Message representing that the batching should finish and that
    /// any remaining batches should be immediately closed and sent to the output.
    Flush,
}

impl<B: Batchable> From<B> for BatchDriverInput<B> {
    fn from(event: B) -> Self {
        BatchDriverInput::Event(event)
    }
}

/// Output message from the BatchDriver's output channel.
#[derive(Debug)]
pub enum BatchDriverOutput<B: Batchable> {
    /// Message representing a batch of items.
    Batch(Vec<B>),
    /// Message representing that batching has finished.
    Flush,
}

impl<B: Batchable> From<BatchDriverOutput<B>> for Vec<B> {
    fn from(value: BatchDriverOutput<B>) -> Self {
        match value {
            BatchDriverOutput::Batch(events) => events,
            BatchDriverOutput::Flush => vec![],
        }
    }
}

/// The central API for using the batching algorithm.
/// Send items in, get batches out.
pub struct BatchDriver<B: Batchable> {
    batcher: Batcher<B>,
    message_box: SimpleMessageBox<BatchDriverInput<B>, BatchDriverOutput<B>>,
    timers: BTreeSet<OffsetDateTime>,
}

enum TimeTo {
    Unbounded,
    Future(std::time::Duration),
    Past(OffsetDateTime),
}

#[async_trait]
impl<B: Batchable> Actor for BatchDriver<B> {
    fn name(&self) -> &str {
        "Event batcher"
    }

    /// Start the batching - runs until receiving a Flush message
    async fn run(&mut self) -> Result<(), RuntimeError> {
        loop {
            let message = match self.time_to_next_timer() {
                TimeTo::Unbounded => self.recv(None),
                TimeTo::Future(timeout) => self.recv(Some(timeout)),
                TimeTo::Past(timer) => {
                    self.timers.remove(&timer);
                    self.time(OffsetDateTime::now_utc()).await?;
                    continue;
                }
            };

            match message.await {
                Err(_) => continue,                         // timer timeout expired
                Ok(None) => break,                          // input channel closed
                Ok(Some(BatchDriverInput::Flush)) => break, // we've been told to stop
                Ok(Some(BatchDriverInput::Event(event))) => self.event(event).await?,
            };
        }

        Ok(self.flush().await?)
    }
}

impl<B: Batchable> BatchDriver<B> {
    /// Define the batching process and channels to interact with it.
    pub fn new(
        batcher: Batcher<B>,
        message_box: SimpleMessageBox<BatchDriverInput<B>, BatchDriverOutput<B>>,
    ) -> BatchDriver<B> {
        BatchDriver {
            batcher,
            message_box,
            timers: BTreeSet::new(),
        }
    }

    async fn recv(
        &mut self,
        timeout: Option<Duration>,
    ) -> Result<Option<BatchDriverInput<B>>, tokio::time::error::Elapsed> {
        match timeout {
            None => Ok(self.message_box.recv().await),
            Some(timeout) => tokio::time::timeout(timeout, self.message_box.recv()).await,
        }
    }

    fn time_to_next_timer(&self) -> TimeTo {
        match self.timers.iter().next() {
            None => TimeTo::Unbounded,
            Some(timer) => {
                let signed_duration = *timer - OffsetDateTime::now_utc();
                if signed_duration.is_negative() {
                    return TimeTo::Past(*timer);
                }
                TimeTo::Future(std::time::Duration::new(
                    signed_duration.abs().whole_seconds() as u64,
                    0,
                ))
            }
        }
    }

    async fn event(&mut self, event: B) -> Result<(), ChannelError> {
        for action in self.batcher.event(OffsetDateTime::now_utc(), event) {
            match action {
                BatcherOutput::Batch(batch) => {
                    self.message_box
                        .send(BatchDriverOutput::Batch(batch))
                        .await?;
                }
                BatcherOutput::Timer(t) => {
                    self.timers.insert(t);
                }
            };
        }

        Ok(())
    }

    async fn time(&mut self, timer: OffsetDateTime) -> Result<(), ChannelError> {
        for batch in self.batcher.time(timer) {
            self.message_box
                .send(BatchDriverOutput::Batch(batch))
                .await?;
        }

        Ok(())
    }

    async fn flush(&mut self) -> Result<(), ChannelError> {
        for batch in self.batcher.flush() {
            self.message_box
                .send(BatchDriverOutput::Batch(batch))
                .await?;
        }

        self.message_box.send(BatchDriverOutput::Flush).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::batchable::Batchable;
    use crate::batcher::Batcher;
    use crate::config::BatchConfigBuilder;
    use crate::driver::BatchDriver;
    use std::time::Duration;
    use tedge_actors::test_helpers::ServiceProviderExt;
    use tedge_actors::Builder;
    use tedge_actors::NoConfig;
    use tedge_actors::SimpleMessageBoxBuilder;
    use tokio::time::timeout;

    type TestBox =
        SimpleMessageBox<BatchDriverOutput<TestBatchEvent>, BatchDriverInput<TestBatchEvent>>;

    #[tokio::test]
    async fn flush_empty() -> Result<(), ChannelError> {
        let mut test_box = spawn_driver();
        test_box.send(BatchDriverInput::Flush).await?;
        assert_recv_flush(&mut test_box).await;
        Ok(())
    }

    #[tokio::test]
    async fn flush_one_batch() -> Result<(), ChannelError> {
        let mut test_box = spawn_driver();

        let event1 = TestBatchEvent::new(1, OffsetDateTime::now_utc());
        test_box.send(BatchDriverInput::Event(event1)).await?;
        test_box.send(BatchDriverInput::Flush).await?;

        assert_recv_batch(&mut test_box, vec![event1]).await;
        assert_recv_flush(&mut test_box).await;

        Ok(())
    }

    #[tokio::test]
    async fn two_batches_with_timer() -> Result<(), ChannelError> {
        let mut test_box = spawn_driver();

        let event1 = TestBatchEvent::new(1, OffsetDateTime::now_utc());
        test_box.send(BatchDriverInput::Event(event1)).await?;

        assert_recv_batch(&mut test_box, vec![event1]).await;

        let event2 = TestBatchEvent::new(2, OffsetDateTime::now_utc());
        test_box.send(BatchDriverInput::Event(event2)).await?;

        assert_recv_batch(&mut test_box, vec![event2]).await;

        Ok(())
    }

    async fn assert_recv_batch(test_box: &mut TestBox, expected: Vec<TestBatchEvent>) {
        match timeout(Duration::from_secs(10), test_box.recv()).await {
            Ok(Some(BatchDriverOutput::Batch(batch))) => assert_batch(batch, expected),
            other => panic!("Failed to receive batch: {:?}", other),
        }
    }

    fn assert_batch(batch: Vec<TestBatchEvent>, expected: Vec<TestBatchEvent>) {
        assert_eq!(batch.len(), expected.len());

        for event in &batch {
            if !expected.contains(event) {
                panic!("Failed to find: {:?}", event);
            }
        }
    }

    async fn assert_recv_flush(test_box: &mut TestBox) {
        match timeout(Duration::from_secs(10), test_box.recv()).await {
            Ok(Some(BatchDriverOutput::Flush)) => {}
            other => panic!("Failed to receive flush: {:?}", other),
        }
    }

    fn spawn_driver() -> TestBox {
        let config = BatchConfigBuilder::new()
            .event_jitter(50)
            .delivery_jitter(20)
            .message_leap_limit(0)
            .build();
        let batcher = Batcher::new(config);
        let mut box_builder = SimpleMessageBoxBuilder::new("test", 1);
        let test_box = box_builder.new_client_box(NoConfig);
        let driver_box = box_builder.build();

        let mut driver = BatchDriver::new(batcher, driver_box);
        tokio::spawn(async move { driver.run().await });

        test_box
    }

    #[derive(Debug, Copy, Clone, Eq, PartialEq)]
    struct TestBatchEvent {
        key: u64,
        event_time: OffsetDateTime,
    }

    impl TestBatchEvent {
        fn new(key: u64, event_time: OffsetDateTime) -> TestBatchEvent {
            TestBatchEvent { key, event_time }
        }
    }

    impl Batchable for TestBatchEvent {
        type Key = u64;

        fn key(&self) -> Self::Key {
            self.key
        }

        fn event_time(&self) -> OffsetDateTime {
            self.event_time
        }
    }
}
