use crate::batchable::Batchable;
use crate::batcher::Batcher;
use crate::batcher::BatcherOutput;
use std::collections::BTreeSet;
use std::time::Duration;
use time::OffsetDateTime;
use tokio::sync::mpsc::error::SendError;
use tokio::sync::mpsc::{Receiver, Sender};

/// Input message to the BatchDriver's input channel.
#[derive(Debug)]
pub enum BatchDriverInput<B: Batchable> {
    /// Message representing a new item to batch.
    Event(B),
    /// Message representing that the batching should finish and that
    /// any remaining batches should be immediately closed and sent to the output.
    Flush,
}

/// Output message from the BatchDriver's output channel.
#[derive(Debug)]
pub enum BatchDriverOutput<B: Batchable> {
    /// Message representing a batch of items.
    Batch(Vec<B>),
    /// Message representing that batching has finished.
    Flush,
}

/// The central API for using the batching algorithm.
/// Send items in, get batches out.
#[derive(Debug)]
pub struct BatchDriver<B: Batchable> {
    batcher: Batcher<B>,
    input: Receiver<BatchDriverInput<B>>,
    output: Sender<BatchDriverOutput<B>>,
    timers: BTreeSet<OffsetDateTime>,
}

enum TimeTo {
    Unbounded,
    Future(std::time::Duration),
    Past(OffsetDateTime),
}

impl<B: Batchable> BatchDriver<B> {
    /// Define the batching process and channels to interact with it.
    pub fn new(
        batcher: Batcher<B>,
        input: Receiver<BatchDriverInput<B>>,
        output: Sender<BatchDriverOutput<B>>,
    ) -> BatchDriver<B> {
        BatchDriver {
            batcher,
            input,
            output,
            timers: BTreeSet::new(),
        }
    }

    /// Start the batching - runs until receiving a Flush message
    pub async fn run(mut self) -> Result<(), SendError<BatchDriverOutput<B>>> {
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

        self.flush().await
    }

    async fn recv(
        &mut self,
        timeout: Option<Duration>,
    ) -> Result<Option<BatchDriverInput<B>>, tokio::time::error::Elapsed> {
        match timeout {
            None => Ok(self.input.recv().await),
            Some(timeout) => tokio::time::timeout(timeout, self.input.recv()).await,
        }
    }

    fn time_to_next_timer(&self) -> TimeTo {
        match self.timers.iter().next() {
            None => TimeTo::Unbounded,
            Some(timer) => {
                let timer2 = timer.clone();
                let signed_duration = timer2 - OffsetDateTime::now_utc();
                if signed_duration.is_negative() {
                    return TimeTo::Past(*timer);
                }
                TimeTo::Future(std::time::Duration::new(signed_duration.abs()))
            }
        }
    }

    async fn event(&mut self, event: B) -> Result<(), SendError<BatchDriverOutput<B>>> {
        for action in self.batcher.event(OffsetDateTime::now_utc(), event) {
            match action {
                BatcherOutput::Batch(batch) => {
                    self.output.send(BatchDriverOutput::Batch(batch)).await?;
                }
                BatcherOutput::Timer(t) => {
                    self.timers.insert(t);
                }
            };
        }

        Ok(())
    }

    async fn time(&mut self, timer: OffsetDateTime) -> Result<(), SendError<BatchDriverOutput<B>>> {
        for batch in self.batcher.time(timer) {
            self.output.send(BatchDriverOutput::Batch(batch)).await?;
        }

        Ok(())
    }

    async fn flush(self) -> Result<(), SendError<BatchDriverOutput<B>>> {
        for batch in self.batcher.flush() {
            self.output.send(BatchDriverOutput::Batch(batch)).await?;
        }

        self.output.send(BatchDriverOutput::Flush).await
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
    use tokio::sync::mpsc::error::SendError;
    use tokio::sync::mpsc::{channel, Receiver, Sender};
    use tokio::time::timeout;

    #[tokio::test]
    async fn flush_empty() -> Result<(), SendError<BatchDriverInput<TestBatchEvent>>> {
        let (input_send, mut output_recv) = spawn_driver();
        input_send.send(BatchDriverInput::Flush).await?;
        assert_recv_flush(&mut output_recv).await;
        Ok(())
    }

    #[tokio::test]
    async fn flush_one_batch() -> Result<(), SendError<BatchDriverInput<TestBatchEvent>>> {
        let (input_send, mut output_recv) = spawn_driver();

        let event1 = TestBatchEvent::new(1, Utc::now());
        input_send.send(BatchDriverInput::Event(event1)).await?;
        input_send.send(BatchDriverInput::Flush).await?;

        assert_recv_batch(&mut output_recv, vec![event1]).await;
        assert_recv_flush(&mut output_recv).await;

        Ok(())
    }

    #[tokio::test]
    async fn two_batches_with_timer() -> Result<(), SendError<BatchDriverInput<TestBatchEvent>>> {
        let (input_send, mut output_recv) = spawn_driver();

        let event1 = TestBatchEvent::new(1, Utc::now());
        input_send.send(BatchDriverInput::Event(event1)).await?;

        assert_recv_batch(&mut output_recv, vec![event1]).await;

        let event2 = TestBatchEvent::new(2, Utc::now());
        input_send.send(BatchDriverInput::Event(event2)).await?;

        assert_recv_batch(&mut output_recv, vec![event2]).await;

        Ok(())
    }

    async fn assert_recv_batch(
        output_recv: &mut Receiver<BatchDriverOutput<TestBatchEvent>>,
        expected: Vec<TestBatchEvent>,
    ) {
        match timeout(Duration::from_secs(10), output_recv.recv()).await {
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

    async fn assert_recv_flush(output_recv: &mut Receiver<BatchDriverOutput<TestBatchEvent>>) {
        match timeout(Duration::from_secs(10), output_recv.recv()).await {
            Ok(Some(BatchDriverOutput::Flush)) => {}
            other => panic!("Failed to receive flush: {:?}", other),
        }
    }

    fn spawn_driver() -> (
        Sender<BatchDriverInput<TestBatchEvent>>,
        Receiver<BatchDriverOutput<TestBatchEvent>>,
    ) {
        let (input_send, input_recv) = channel(1);
        let (output_send, output_recv) = channel(1);
        let config = BatchConfigBuilder::new()
            .event_jitter(50)
            .delivery_jitter(20)
            .message_leap_limit(0)
            .build();
        let batcher = Batcher::new(config);

        let driver = BatchDriver::new(batcher, input_recv, output_send);
        tokio::spawn(driver.run());

        (input_send, output_recv)
    }

    #[derive(Debug, Copy, Clone, Eq, PartialEq)]
    struct TestBatchEvent {
        key: u64,
        event_time: DateTime<Utc>,
    }

    impl TestBatchEvent {
        fn new(key: u64, event_time: DateTime<Utc>) -> TestBatchEvent {
            TestBatchEvent { key, event_time }
        }
    }

    impl Batchable for TestBatchEvent {
        type Key = u64;

        fn key(&self) -> Self::Key {
            self.key
        }

        fn event_time(&self) -> DateTime<Utc> {
            self.event_time
        }
    }
}
