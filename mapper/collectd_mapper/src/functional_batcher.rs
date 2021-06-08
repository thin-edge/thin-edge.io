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
/*
pub enum Output {
    /// The imperative shell has nothing to do.
    Nop,

    /// Informs the imperative shell to send a `Input::Tick` at (or slightly after) the specified timestamp.
    NextTickAt(Timestamp),

    /// Informs the imperative shell to send a message batch out.
    MessageBatch(MessageBatch),
}
*/

#[derive(Debug, PartialEq)]
pub struct Output {
    /// Informs the imperative shell to send a `Input::Tick` at (or slightly after) the specified timestamp.
    next_tick_at: Option<Timestamp>,

    /// Informs the imperative shell to send a message batch out.
    message_batch: Option<MessageBatch>,
}

impl Batcher {
    pub fn new(
        max_batch_size: usize,
        max_batch_age: chrono::Duration,
        collectd_timestamp_delta: f64,
    ) -> Self {
        assert!(max_batch_size > 0);
        Self {
            max_batch_size,
            max_batch_age,
            collectd_timestamp_delta,
            current_batch: CurrentBatch::empty(),
        }
    }

    pub fn handle(&mut self, input: Input) -> Output {
        match input {
            Input::Message {
                message,
                received_at,
            } => self.handle_message(message, received_at),
            Input::Flush => self.handle_flush(),
            _ => {
                unimplemented!()
            }
        }
    }

    fn handle_message(&mut self, message: OwnedCollectdMessage, received_at: Timestamp) -> Output {
        let mut output = Output {
            next_tick_at: None,
            message_batch: None,
        };
        self.current_batch.messages.push(message);
        self.current_batch.opened_at = Some(self.current_batch.opened_at.unwrap_or(received_at));

        if self.current_batch.messages.len() >= self.max_batch_size {
            let last_batch = std::mem::replace(&mut self.current_batch, CurrentBatch::empty());
            output.message_batch = Some(MessageBatch(last_batch.messages));
        }
        output
    }

    fn handle_flush(&mut self) -> Output {
        if self.current_batch.is_empty() {
            Output {
                next_tick_at: None,
                message_batch: None,
            }
        } else {
            let last_batch = std::mem::replace(&mut self.current_batch, CurrentBatch::empty());
            Output {
                next_tick_at: None,
                message_batch: Some(MessageBatch(last_batch.messages)),
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

    let expected_outputs = vec![
        Output {
            next_tick_at: None,
            message_batch: None,
        },
        Output {
            next_tick_at: None,
            message_batch: None,
        },
        Output {
            next_tick_at: None,
            message_batch: Some(MessageBatch(messages)),
        },
        Output {
            next_tick_at: None,
            message_batch: None,
        },
    ];

    test_batcher(&mut batcher, inputs, expected_outputs);
}

#[cfg(test)]
fn test_batcher(batcher: &mut Batcher, inputs: Vec<Input>, expected_outputs: Vec<Output>) {
    let outputs: Vec<_> = inputs
        .into_iter()
        .map(|input| batcher.handle(input))
        .collect();

    assert_eq!(outputs, expected_outputs);
}
