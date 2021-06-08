use crate::collectd::OwnedCollectdMessage;
/// IO-less batching algorithm. Imperative shell, functional core.
use clock::Timestamp;

#[derive(Debug, PartialEq)]
pub struct MessageBatch(pub Vec<OwnedCollectdMessage>);

/// The Batcher's internal state.
pub struct Batcher {
    /// Maximum number of messages per batch.
    max_batch_size: usize,

    /// The maximum age of a batch.
    max_batch_age: chrono::Duration,

    /// We start a new batch upon receiving a message whose timestamp is farther away to the
    /// timestamp of first messsge in the batch than `collectd_timestamp_delta` seconds.
    /// Delta is inclusive.
    collectd_timestamp_delta: f64,

    current_batch: CurrentBatch,
}

struct CurrentBatch {
    opened_at: Option<Timestamp>,
    messages: Vec<OwnedCollectdMessage>,
}

impl CurrentBatch {
    fn empty() -> Self {
        Self {
            opened_at: None,
            messages: Vec::new(),
        }
    }

    fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }
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
    /// Informs the imperative shell to send a `Input::Tick` at (or slightly after) the specified timestamp.
    NextTickAt(Timestamp),

    /// Informs the imperative shell to send a message batch out.
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
            current_batch: CurrentBatch::empty(),
        }
    }

    pub fn handle(&mut self, input: Input, outputs: &mut Vec<Output>) {
        match input {
            Input::Message {
                message,
                received_at,
            } => self.handle_message(message, received_at, outputs),
            Input::Flush => self.handle_flush(outputs),
            _ => {
                unimplemented!()
            }
        }
    }

    fn handle_message(
        &mut self,
        message: OwnedCollectdMessage,
        received_at: Timestamp,
        outputs: &mut Vec<Output>,
    ) {
        if !self.message_in_delta(&message) {
            self.handle_flush(outputs);
        }

        self.current_batch.messages.push(message);
        self.current_batch.opened_at = Some(self.current_batch.opened_at.unwrap_or(received_at));

        if self.current_batch.messages.len() >= self.max_batch_size {
            self.handle_flush(outputs);
        }
    }

    fn handle_flush(&mut self, outputs: &mut Vec<Output>) {
        if !self.current_batch.is_empty() {
            let last_batch = std::mem::replace(&mut self.current_batch, CurrentBatch::empty());
            outputs.push(Output::MessageBatch(MessageBatch(last_batch.messages)));
        }
    }

    fn message_in_delta(&self, message: &OwnedCollectdMessage) -> bool {
        match self.current_batch.messages.first() {
            None => true,
            Some(first) => {
                (first.timestamp() - message.timestamp()).abs() <= self.collectd_timestamp_delta
            }
        }
    }
}

#[test]
fn it_batches_messages_until_max_batch_size_is_reached() {
    use crate::collectd::CollectdMessage;
    use clock::Clock;

    let fixed_timestamp = clock::WallClock.now();

    let mut batcher = Batcher::new(3, chrono::Duration::hours(1), 10.0);

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

    let expected_outputs = vec![Output::MessageBatch(MessageBatch(messages))];

    test_batcher(&mut batcher, inputs, expected_outputs);
}

#[test]
fn it_batches_messages_within_collectd_timestamp_delta() {
    use crate::collectd::CollectdMessage;
    use clock::Clock;

    let fixed_timestamp = clock::WallClock.now();

    let mut batcher = Batcher::new(1000, chrono::Duration::hours(1), 1.5);

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
        Output::MessageBatch(MessageBatch(vec![messages[0].clone(), messages[1].clone()])),
        Output::MessageBatch(MessageBatch(vec![messages[2].clone(), messages[3].clone()])),
        Output::MessageBatch(MessageBatch(vec![messages[4].clone()])),
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
