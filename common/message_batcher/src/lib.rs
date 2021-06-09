//! Batching algorithm which is unaware of IO. It uses the "Imperative
//! shell, functional core" approach with this code being the
//! "functional core".

pub type Timestamp = chrono::DateTime<chrono::FixedOffset>;

/// A batch of messages. Guaranteed to contain at least one message.
#[derive(Debug, PartialEq)]
pub struct MessageBatch<T> {
    opened_at: Timestamp,
    messages: Vec<T>,
}

impl<T> MessageBatch<T> {
    pub fn new(opened_at: Timestamp, first_message: T) -> Self {
        Self {
            opened_at,
            messages: vec![first_message],
        }
    }

    pub fn opened_at(&self) -> Timestamp {
        self.opened_at
    }

    pub fn iter_messages(&self) -> impl Iterator<Item = &T> {
        self.messages.iter()
    }

    pub fn first(&self) -> &T {
        &self.messages[0]
    }
}

/// Decision whether to put a message into the current batch or start a new one.
pub trait BatchingCriterion<T>: Send {
    /// Returns true, if the message belongs to the current batch.
    fn belongs_to_batch(&self, message: &T, message_batch: &MessageBatch<T>) -> bool;
}

/// Inputs to the `MessageBatcher`.
pub enum Input<T> {
    /// A message was received.
    Message {
        /// Time the message has been received.
        received_at: Timestamp,

        /// The message itself.
        message: T,
    },

    /// Notify the `MessageBatcher` about an expired timer, requested through `Output::NotifyAt`.
    ///
    /// Allows the `MessageBatcher` to close a batch when the current time window has expired.
    Notify {
        /// The current system time.
        now: Timestamp,
    },

    /// Will flush the current batch. This is used upon termination.
    Flush,
}

/// Outputs from the `MessageBatcher`. These inform the imperative shell to perform some actions on behalf
/// of the `MessageBatcher`.
#[derive(Debug, PartialEq)]
pub enum Output<T> {
    /// Informs the imperative shell to send an `Input::Notify` at (or slightly after) the specified timestamp.
    NotifyAt(Timestamp),

    /// Informs the imperative shell to send the message batch out.
    MessageBatch(MessageBatch<T>),
}

/// The MessageBatcher's internal state / configuration.
pub struct MessageBatcher<T> {
    /// Maximum number of messages per batch.
    max_batch_size: usize,

    /// The maximum age of a batch.
    ///
    /// Age of a batch is the elapsed time since `current_batch.opened_at`.
    max_batch_age: chrono::Duration,

    /// The decisions whether a message belongs to the current batch or not.
    batching_criteria: Vec<Box<dyn BatchingCriterion<T>>>,

    current_batch: Option<MessageBatch<T>>,
}

impl<T> MessageBatcher<T> {
    pub fn new(max_batch_size: usize, max_batch_age: chrono::Duration) -> Self {
        assert!(max_batch_size > 0);
        Self {
            max_batch_size,
            max_batch_age,
            batching_criteria: Vec::new(),
            current_batch: None,
        }
    }

    pub fn with_batching_criterion(
        mut self,
        batching_criterion: impl BatchingCriterion<T> + 'static,
    ) -> Self {
        self.batching_criteria.push(Box::new(batching_criterion));
        self
    }

    pub fn handle(&mut self, input: Input<T>, outputs: &mut Vec<Output<T>>) {
        match input {
            Input::Message {
                message,
                received_at,
            } => self.handle_message(message, received_at, outputs),
            Input::Notify { now } => self.handle_notify(now, outputs),
            Input::Flush => self.handle_flush(outputs),
        }

        // Inform imperative shell about when to send a `Input::Notify` message.
        if let Some(batch_opened_at) = self.current_batch.as_ref().map(|batch| batch.opened_at) {
            outputs.push(Output::NotifyAt(batch_opened_at + self.max_batch_age));
        }
    }

    fn handle_message(&mut self, message: T, received_at: Timestamp, outputs: &mut Vec<Output<T>>) {
        if self.timestamp_exceeds_max_age(received_at)
            || !self.message_belongs_to_current_batch(&message)
        {
            // the current message starts a new batch.
            self.handle_flush(outputs);
        }

        match self.current_batch {
            Some(ref mut current_batch) => {
                current_batch.messages.push(message);
            }
            None => {
                self.current_batch = Some(MessageBatch::new(received_at, message));
            }
        }

        if self.current_batch_size() >= self.max_batch_size {
            self.handle_flush(outputs);
        }
    }

    fn handle_notify(&mut self, now: Timestamp, outputs: &mut Vec<Output<T>>) {
        if self.timestamp_exceeds_max_age(now) {
            self.handle_flush(outputs);
        }
    }

    fn handle_flush(&mut self, outputs: &mut Vec<Output<T>>) {
        if let Some(last_batch) = self.current_batch.take() {
            outputs.push(Output::MessageBatch(last_batch));
        }
    }

    fn message_belongs_to_current_batch(&self, message: &T) -> bool {
        match self.current_batch.as_ref() {
            Some(current_batch) => self
                .batching_criteria
                .iter()
                .all(|crit| crit.belongs_to_batch(message, current_batch)),
            None => true,
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

#[cfg(test)]
#[derive(Clone, Debug, PartialEq)]
struct CollectdMessage {
    group_key: String,
    key: String,
    value: f64,
    timestamp: f64,
}

#[cfg(test)]
impl CollectdMessage {
    fn new(
        group_key: impl Into<String>,
        key: impl Into<String>,
        value: f64,
        timestamp: f64,
    ) -> Self {
        Self {
            group_key: group_key.into(),
            key: key.into(),
            value,
            timestamp,
        }
    }
}

#[test]
fn it_batches_messages_until_max_batch_size_is_reached() {
    use chrono::{prelude::*, Duration};

    let fixed_timestamp = FixedOffset::east(7 * 3600)
        .ymd(2014, 7, 8)
        .and_hms(9, 10, 11);

    let one_hour = Duration::hours(1);

    let mut batcher = MessageBatcher::new(3, one_hour);

    let messages = vec![
        CollectdMessage::new("coordinate", "z", 90.0, 1.0),
        CollectdMessage::new("coordinate", "z", 90.0, 1.0),
        CollectdMessage::new("coordinate", "z", 90.0, 1.0),
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
        Output::NotifyAt(fixed_timestamp + one_hour),
        Output::NotifyAt(fixed_timestamp + one_hour),
        Output::MessageBatch(MessageBatch {
            opened_at: fixed_timestamp,
            messages,
        }),
    ];

    test_batcher(&mut batcher, inputs, expected_outputs);
}

#[test]
fn it_batches_messages_within_collectd_timestamp_delta() {
    use chrono::prelude::*;

    let fixed_timestamp = FixedOffset::east(7 * 3600)
        .ymd(2014, 7, 8)
        .and_hms(9, 10, 11);

    let one_hour = chrono::Duration::hours(1);

    /// We start a new batch upon receiving a message whose timestamp is farther away to the
    /// timestamp of first message in the batch than `delta` seconds.
    ///
    /// Delta is inclusive.
    struct CollectdTimestampDeltaCriterion {
        delta: f64,
    }

    impl BatchingCriterion<CollectdMessage> for CollectdTimestampDeltaCriterion {
        fn belongs_to_batch(
            &self,
            message: &CollectdMessage,
            message_batch: &MessageBatch<CollectdMessage>,
        ) -> bool {
            let delta = message_batch.first().timestamp - message.timestamp;
            delta.abs() <= self.delta
        }
    }

    let mut batcher = MessageBatcher::new(1000, one_hour)
        .with_batching_criterion(CollectdTimestampDeltaCriterion { delta: 1.5 });

    let messages = vec![
        CollectdMessage::new("coordinate", "z", 90.0, 0.0),
        CollectdMessage::new("coordinate", "z", 90.0, 1.0),
        CollectdMessage::new("coordinate", "z", 90.0, 2.0),
        CollectdMessage::new("coordinate", "z", 90.0, 3.0),
        CollectdMessage::new("coordinate", "z", 90.0, 4.0),
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
        Output::NotifyAt(fixed_timestamp + one_hour),
        Output::NotifyAt(fixed_timestamp + one_hour),
        Output::MessageBatch(MessageBatch {
            opened_at: fixed_timestamp,
            messages: vec![messages[0].clone(), messages[1].clone()],
        }),
        Output::NotifyAt(fixed_timestamp + one_hour),
        Output::NotifyAt(fixed_timestamp + one_hour),
        Output::MessageBatch(MessageBatch {
            opened_at: fixed_timestamp,
            messages: vec![messages[2].clone(), messages[3].clone()],
        }),
        Output::NotifyAt(fixed_timestamp + one_hour),
        Output::MessageBatch(MessageBatch {
            opened_at: fixed_timestamp,
            messages: vec![messages[4].clone()],
        }),
    ];

    test_batcher(&mut batcher, inputs, expected_outputs);
}

#[test]
fn it_batches_messages_based_on_max_age() {
    use chrono::{prelude::*, Duration};

    let fixed_timestamp = FixedOffset::east(7 * 3600)
        .ymd(2014, 7, 8)
        .and_hms(9, 10, 11);

    let ten_seconds = Duration::seconds(10);

    let mut batcher = MessageBatcher::new(1000, ten_seconds);

    let messages = vec![
        CollectdMessage::new("coordinate", "z", 90.0, 0.0),
        CollectdMessage::new("coordinate", "z", 90.0, 1.0),
        CollectdMessage::new("coordinate", "z", 90.0, 2.0),
        CollectdMessage::new("coordinate", "z", 90.0, 3.0),
        CollectdMessage::new("coordinate", "z", 90.0, 4.0),
        CollectdMessage::new("coordinate", "z", 90.0, 5.0),
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
            received_at: fixed_timestamp + Duration::seconds(9),
            message: messages[2].clone(),
        },
        Input::Message {
            received_at: fixed_timestamp + Duration::seconds(11),
            message: messages[3].clone(),
        },
        Input::Message {
            received_at: fixed_timestamp + Duration::milliseconds(20999),
            message: messages[4].clone(),
        },
        Input::Message {
            received_at: fixed_timestamp + Duration::seconds(21),
            message: messages[5].clone(),
        },
        Input::Flush,
    ];

    let expected_outputs = vec![
        Output::NotifyAt(fixed_timestamp + ten_seconds),
        Output::NotifyAt(fixed_timestamp + ten_seconds),
        Output::NotifyAt(fixed_timestamp + ten_seconds),
        Output::MessageBatch(MessageBatch {
            opened_at: fixed_timestamp,
            messages: vec![
                messages[0].clone(),
                messages[1].clone(),
                messages[2].clone(),
            ],
        }),
        Output::NotifyAt(fixed_timestamp + Duration::seconds(11) + ten_seconds),
        Output::NotifyAt(fixed_timestamp + Duration::seconds(11) + ten_seconds),
        Output::MessageBatch(MessageBatch {
            opened_at: fixed_timestamp + Duration::seconds(11),
            messages: vec![messages[3].clone(), messages[4].clone()],
        }),
        Output::NotifyAt(fixed_timestamp + Duration::seconds(21) + ten_seconds),
        Output::MessageBatch(MessageBatch {
            opened_at: fixed_timestamp + Duration::seconds(21),
            messages: vec![messages[5].clone()],
        }),
    ];

    test_batcher(&mut batcher, inputs, expected_outputs);
}

#[cfg(test)]
fn test_batcher(
    batcher: &mut MessageBatcher<CollectdMessage>,
    inputs: Vec<Input<CollectdMessage>>,
    expected_outputs: Vec<Output<CollectdMessage>>,
) {
    let mut outputs = Vec::new();
    inputs
        .into_iter()
        .for_each(|input| batcher.handle(input, &mut outputs));

    assert_eq!(outputs, expected_outputs);
}
