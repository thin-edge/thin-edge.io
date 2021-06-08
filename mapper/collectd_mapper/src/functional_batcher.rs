//! Batching algorithm that is unaware of IO. Imperative shell, functional core.

use crate::collectd::OwnedCollectdMessage;
use clock::Timestamp;

#[derive(Debug, PartialEq)]
pub struct MessageBatch {
    pub opened_at: Timestamp,
    pub messages: Vec<OwnedCollectdMessage>,
}

/// The Batcher's internal state / configuration.
pub struct Batcher {
    /// Maximum number of messages per batch.
    max_batch_size: usize,

    /// The maximum age of a batch.
    ///
    /// Age of a batch is the elapsed time since `current_batch.opened_at`.
    max_batch_age: chrono::Duration,

    /// We start a new batch upon receiving a message whose timestamp is farther away to the
    /// timestamp of first messsge in the batch than `collectd_timestamp_delta` seconds.
    ///
    /// Delta is inclusive.
    collectd_timestamp_delta: f64,

    current_batch: Option<MessageBatch>,
}

/// Inputs to the `Batcher`.
pub enum Input {
    /// A message was received.
    Message {
        /// Time the message has been received.
        received_at: Timestamp,

        /// The message itself.
        message: OwnedCollectdMessage,
    },

    /// Time has progressed.
    ///
    /// Allows the Batcher to close a batch when the current time window has expired.
    Tick {
        /// The current system time.
        now: Timestamp,
    },

    /// Will flush the current batch. This is used upon termination.
    Flush,
}

/// Outputs from the `Batcher`. These inform the imperative shell to perform some actions on behalf
/// of the `Batcher`.
#[derive(Debug, PartialEq)]
pub enum Output {
    /// Informs the imperative shell to send an `Input::Tick` at (or slightly after) the specified timestamp.
    NextTickAt(Timestamp),

    /// Informs the imperative shell to send the message batch out.
    MessageBatch(MessageBatch),
}

impl Batcher {
    pub fn new(
        max_batch_size: usize,
        max_batch_age: chrono::Duration,
        collectd_timestamp_delta: f64,
    ) -> Self {
        assert!(max_batch_size > 0);
        assert!(collectd_timestamp_delta >= 0.0);
        Self {
            max_batch_size,
            max_batch_age,
            collectd_timestamp_delta,
            current_batch: None,
        }
    }

    pub fn handle(&mut self, input: Input, outputs: &mut Vec<Output>) {
        match input {
            Input::Message {
                message,
                received_at,
            } => self.handle_message(message, received_at, outputs),
            Input::Tick { now } => self.handle_tick(now, outputs),
            Input::Flush => self.handle_flush(outputs),
        }

        // Inform imperative shell about when to send a Tick message.
        if let Some(batch_opened_at) = self.current_batch.as_ref().map(|batch| batch.opened_at) {
            outputs.push(Output::NextTickAt(batch_opened_at + self.max_batch_age));
        }
    }

    fn handle_message(
        &mut self,
        message: OwnedCollectdMessage,
        received_at: Timestamp,
        outputs: &mut Vec<Output>,
    ) {
        if self.message_exceeds_delta(&message) || self.timestamp_exceeds_max_age(received_at) {
            // the current message starts a new batch.
            self.handle_flush(outputs);
        }

        match self.current_batch {
            Some(ref mut current_batch) => {
                debug_assert!(current_batch.messages.len() > 0);
                current_batch.messages.push(message);
            }
            None => {
                self.current_batch = Some(MessageBatch {
                    opened_at: received_at,
                    messages: vec![message],
                });
            }
        }

        if self.current_batch_size() >= self.max_batch_size {
            self.handle_flush(outputs);
        }
    }

    fn handle_tick(&mut self, now: Timestamp, outputs: &mut Vec<Output>) {
        if self.timestamp_exceeds_max_age(now) {
            self.handle_flush(outputs);
        }
    }

    fn handle_flush(&mut self, outputs: &mut Vec<Output>) {
        if let Some(last_batch) = self.current_batch.take() {
            outputs.push(Output::MessageBatch(last_batch));
        }
    }

    fn message_exceeds_delta(&self, message: &OwnedCollectdMessage) -> bool {
        match self
            .current_batch
            .as_ref()
            .and_then(|batch| batch.messages.first())
        {
            None => false,
            Some(first) => {
                let delta = (first.timestamp() - message.timestamp()).abs();
                delta > self.collectd_timestamp_delta
            }
        }
    }

    fn timestamp_exceeds_max_age(&self, timestamp: Timestamp) -> bool {
        match self.current_batch {
            Some(MessageBatch { opened_at, .. }) => {
                let age = timestamp - opened_at;
                age >= self.max_batch_age
            }
            None => false,
        }
    }

    fn current_batch_size(&self) -> usize {
        self.current_batch
            .as_ref()
            .map(|batch| batch.messages.len())
            .unwrap_or(0)
    }
}

#[cfg(test)]
use pretty_assertions::assert_eq;

#[test]
fn it_batches_messages_until_max_batch_size_is_reached() {
    use crate::collectd::CollectdMessage;
    use clock::Clock;

    let fixed_timestamp = clock::WallClock.now();
    let one_hour = chrono::Duration::hours(1);

    let mut batcher = Batcher::new(3, one_hour, 10.0);

    let messages: Vec<OwnedCollectdMessage> = vec![
        CollectdMessage::new("coordinate", "z", 90.0, 1.0).into(),
        CollectdMessage::new("coordinate", "z", 90.0, 1.0).into(),
        CollectdMessage::new("coordinate", "z", 90.0, 1.0).into(),
    ];

    let inputs = vec![
        Input::Message {
            received_at: fixed_timestamp,
            message: messages[0].clone(),
        },
        Input::Message {
            received_at: fixed_timestamp,
            message: messages[1].clone(),
        },
        Input::Message {
            received_at: fixed_timestamp,
            message: messages[2].clone(),
        },
        Input::Flush,
    ];

    let expected_outputs = vec![
        Output::NextTickAt(fixed_timestamp + one_hour),
        Output::NextTickAt(fixed_timestamp + one_hour),
        Output::MessageBatch(MessageBatch {
            opened_at: fixed_timestamp,
            messages,
        }),
    ];

    test_batcher(&mut batcher, inputs, expected_outputs);
}

#[test]
fn it_batches_messages_within_collectd_timestamp_delta() {
    use crate::collectd::CollectdMessage;
    use clock::Clock;

    let fixed_timestamp = clock::WallClock.now();
    let one_hour = chrono::Duration::hours(1);

    let mut batcher = Batcher::new(1000, one_hour, 1.5);

    let messages: Vec<OwnedCollectdMessage> = vec![
        CollectdMessage::new("coordinate", "z", 90.0, 0.0).into(),
        CollectdMessage::new("coordinate", "z", 90.0, 1.0).into(),
        CollectdMessage::new("coordinate", "z", 90.0, 2.0).into(),
        CollectdMessage::new("coordinate", "z", 90.0, 3.0).into(),
        CollectdMessage::new("coordinate", "z", 90.0, 4.0).into(),
    ];

    let inputs = vec![
        Input::Message {
            received_at: fixed_timestamp,
            message: messages[0].clone(),
        },
        Input::Message {
            received_at: fixed_timestamp,
            message: messages[1].clone(),
        },
        Input::Message {
            received_at: fixed_timestamp,
            message: messages[2].clone(),
        },
        Input::Message {
            received_at: fixed_timestamp,
            message: messages[3].clone(),
        },
        Input::Message {
            received_at: fixed_timestamp,
            message: messages[4].clone(),
        },
        Input::Flush,
    ];

    let expected_outputs = vec![
        Output::NextTickAt(fixed_timestamp + one_hour),
        Output::NextTickAt(fixed_timestamp + one_hour),
        Output::MessageBatch(MessageBatch {
            opened_at: fixed_timestamp,
            messages: vec![messages[0].clone(), messages[1].clone()],
        }),
        Output::NextTickAt(fixed_timestamp + one_hour),
        Output::NextTickAt(fixed_timestamp + one_hour),
        Output::MessageBatch(MessageBatch {
            opened_at: fixed_timestamp,
            messages: vec![messages[2].clone(), messages[3].clone()],
        }),
        Output::NextTickAt(fixed_timestamp + one_hour),
        Output::MessageBatch(MessageBatch {
            opened_at: fixed_timestamp,
            messages: vec![messages[4].clone()],
        }),
    ];

    test_batcher(&mut batcher, inputs, expected_outputs);
}

#[test]
fn it_batches_messages_based_on_max_age() {
    use crate::collectd::CollectdMessage;
    use chrono::Duration;
    use clock::Clock;

    let fixed_timestamp = clock::WallClock.now();
    let ten_seconds = Duration::seconds(10);

    let mut batcher = Batcher::new(1000, ten_seconds, 100000.0);

    let messages: Vec<OwnedCollectdMessage> = vec![
        CollectdMessage::new("coordinate", "z", 90.0, 0.0).into(),
        CollectdMessage::new("coordinate", "z", 90.0, 1.0).into(),
        CollectdMessage::new("coordinate", "z", 90.0, 2.0).into(),
        CollectdMessage::new("coordinate", "z", 90.0, 3.0).into(),
        CollectdMessage::new("coordinate", "z", 90.0, 4.0).into(),
        CollectdMessage::new("coordinate", "z", 90.0, 5.0).into(),
    ];

    let inputs = vec![
        Input::Message {
            received_at: fixed_timestamp,
            message: messages[0].clone(),
        },
        Input::Message {
            received_at: fixed_timestamp,
            message: messages[1].clone(),
        },
        Input::Message {
            received_at: fixed_timestamp + chrono::Duration::seconds(9),
            message: messages[2].clone(),
        },
        Input::Message {
            received_at: fixed_timestamp + chrono::Duration::seconds(11),
            message: messages[3].clone(),
        },
        Input::Message {
            received_at: fixed_timestamp + chrono::Duration::milliseconds(20999),
            message: messages[4].clone(),
        },
        Input::Message {
            received_at: fixed_timestamp + chrono::Duration::seconds(21),
            message: messages[5].clone(),
        },
        Input::Flush,
    ];

    let expected_outputs = vec![
        Output::NextTickAt(fixed_timestamp + ten_seconds),
        Output::NextTickAt(fixed_timestamp + ten_seconds),
        Output::NextTickAt(fixed_timestamp + ten_seconds),
        Output::MessageBatch(MessageBatch {
            opened_at: fixed_timestamp,
            messages: vec![
                messages[0].clone(),
                messages[1].clone(),
                messages[2].clone(),
            ],
        }),
        Output::NextTickAt(fixed_timestamp + Duration::seconds(11) + ten_seconds),
        Output::NextTickAt(fixed_timestamp + Duration::seconds(11) + ten_seconds),
        Output::MessageBatch(MessageBatch {
            opened_at: fixed_timestamp + Duration::seconds(11),
            messages: vec![messages[3].clone(), messages[4].clone()],
        }),
        Output::NextTickAt(fixed_timestamp + Duration::seconds(21) + ten_seconds),
        Output::MessageBatch(MessageBatch {
            opened_at: fixed_timestamp + Duration::seconds(21),
            messages: vec![messages[5].clone()],
        }),
    ];

    test_batcher(&mut batcher, inputs, expected_outputs);
}

#[cfg(test)]
fn test_batcher(batcher: &mut Batcher, inputs: Vec<Input>, expected_outputs: Vec<Output>) {
    let mut outputs = Vec::new();
    inputs
        .into_iter()
        .for_each(|input| batcher.handle(input, &mut outputs));

    assert_eq!(outputs, expected_outputs);
}
