//! Batching algorithm which is unaware of IO. It uses the "Imperative
//! shell, functional core" approach with this code being the
//! "functional core".

pub mod dedup;
pub mod filter;
pub mod group;

pub type Timestamp = chrono::DateTime<chrono::FixedOffset>;
pub type Duration = chrono::Duration;

pub use filter::MessageFilter;
pub use group::{MessageGroup, MessageGrouper};

/// Inputs to the `MessageBatcher`.
pub enum Input<T> {
    /// A message was received.
    Message(T),

    /// Notify the `MessageBatcher` about the current system time.
    ///
    /// This is the result of either an expired timer, requested through `Output::NextTickAt`
    /// or a change in the current system time (e.g. when a message was received).
    ///
    /// Allows the `MessageBatcher` to close a batch when the current time window has expired.
    Tick(Timestamp),

    /// Will flush the current batch. This is used upon termination.
    Flush,
}

/// Outputs from the `MessageBatcher`. These inform the imperative shell to perform some actions on behalf
/// of the `MessageBatcher`.
#[derive(Debug, PartialEq)]
pub enum Output<T: Send> {
    /// Informs the imperative shell to send an `Input::Notify` at (or slightly after) the specified timestamp.
    NextTickAt(Timestamp),

    /// Informs the imperative shell to send the message batch out.
    MessageBatch(MessageGroup<T>),
}

/// The MessageBatcher's internal state / configuration.
pub struct MessageBatcher<T: Send + Clone> {
    /// Message filter to reject messages before grouping them.
    message_filter: Box<dyn MessageFilter<T>>,

    /// The message grouper
    message_grouper: MessageGrouper<T>,

    ///
    max_batch_age: Duration,

    /// Current system time
    current_timestamp: Timestamp,
}

impl<T: Clone + Send> MessageBatcher<T> {
    pub fn new(
        message_filter: Box<dyn MessageFilter<T>>,
        message_grouper: MessageGrouper<T>,
        max_batch_age: Duration,
        startup_time: Timestamp,
    ) -> Self {
        Self {
            message_filter,
            message_grouper,
            max_batch_age,
            current_timestamp: startup_time,
        }
    }

    pub fn handle_vec(&mut self, input: Input<T>) -> Vec<Output<T>> {
        let mut outputs = Vec::new();
        self.handle(input, &mut outputs);
        outputs
    }

    pub fn handle(&mut self, input: Input<T>, outputs: &mut Vec<Output<T>>) {
        match input {
            Input::Tick(timestamp) => self.handle_tick(timestamp, outputs),
            Input::Message(message) => self.handle_message(message, outputs),
            Input::Flush => self.handle_flush(outputs),
        }

        // Inform imperative shell about when to send the next `Input::Tick` message.
        if let Some(min_created_at) = self.message_grouper.min_created_at() {
            outputs.push(Output::NextTickAt(min_created_at + self.max_batch_age));
        }
    }

    fn handle_tick(&mut self, timestamp: Timestamp, outputs: &mut Vec<Output<T>>) {
        self.current_timestamp = timestamp;
        self.retire_messages(outputs);
    }

    fn handle_message(&mut self, message: T, outputs: &mut Vec<Output<T>>) {
        match self.message_filter.filter(&message) {
            filter::FilterDecision::Accept => {
                self.message_grouper
                    .group_message(message, self.current_timestamp);
                self.retire_messages(outputs);
            }
            filter::FilterDecision::Reject => {
                // ignore message
            }
        }
    }

    fn handle_flush(&mut self, outputs: &mut Vec<Output<T>>) {
        for message_group in self.message_grouper.flush_groups().into_iter() {
            outputs.push(Output::MessageBatch(message_group));
        }
    }

    fn retire_messages(&mut self, outputs: &mut Vec<Output<T>>) {
        for message_group in self
            .message_grouper
            .retire_groups(self.current_timestamp)
            .into_iter()
        {
            outputs.push(Output::MessageBatch(message_group));
        }
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

/// We start a new batch upon receiving a message whose timestamp is farther away to the
/// timestamp of first message in the batch than `delta` seconds.
///
/// Delta is inclusive.
#[cfg(test)]
struct CollectdTimestampDeltaCriterion {
    delta: f64,
}

#[cfg(test)]
impl group::IsGroupMember<CollectdMessage> for CollectdTimestampDeltaCriterion {
    fn is_group_member(
        &self,
        message: &CollectdMessage,
        group: &MessageGroup<CollectdMessage>,
    ) -> bool {
        let delta = group.first().timestamp - message.timestamp;
        delta.abs() <= self.delta
    }
}

#[cfg(test)]
struct RetirementPolicy {
    max_group_age: Duration,
}

// XXX: Rename CanRetire -> RetirementPolicy
#[cfg(test)]
impl group::CanRetire<CollectdMessage> for RetirementPolicy {
    fn can_retire(&self, group: &MessageGroup<CollectdMessage>, now: Timestamp) -> bool {
        let age = now - group.created_at();
        age >= self.max_group_age
    }
}

#[test]
fn it_batches_messages_until_max_batch_size_is_reached() {
    use chrono::{prelude::*, Duration};

    let fixed_timestamp = FixedOffset::east(7 * 3600)
        .ymd(2014, 7, 8)
        .and_hms(9, 10, 11);

    let one_hour = Duration::hours(1);

    let mut batcher = MessageBatcher::new(
        Box::new(filter::NoMessageFilter::new()),
        group::MessageGrouper::new(
            Box::new(CollectdTimestampDeltaCriterion { delta: 3.0 }),
            Box::new(RetirementPolicy {
                max_group_age: one_hour,
            }),
        ),
        one_hour,
        fixed_timestamp,
    );

    let messages = vec![
        CollectdMessage::new("coordinate", "z", 90.0, 1.0),
        CollectdMessage::new("coordinate", "z", 90.0, 1.0),
        CollectdMessage::new("coordinate", "z", 90.0, 1.0),
    ];

    let inputs = vec![
        Input::Tick(fixed_timestamp),
        Input::Message(messages[0].clone()),
        Input::Message(messages[1].clone()),
        Input::Message(messages[2].clone()),
        Input::Flush,
    ];

    let expected_outputs = vec![
        Output::NextTickAt(fixed_timestamp + one_hour),
        Output::NextTickAt(fixed_timestamp + one_hour),
        Output::NextTickAt(fixed_timestamp + one_hour),
        Output::MessageBatch(MessageGroup::from_messages(messages, fixed_timestamp)),
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

    let mut batcher = MessageBatcher::new(
        Box::new(filter::NoMessageFilter::new()),
        group::MessageGrouper::new(
            Box::new(CollectdTimestampDeltaCriterion { delta: 1.5 }),
            Box::new(RetirementPolicy {
                max_group_age: one_hour,
            }),
        ),
        one_hour,
        fixed_timestamp,
    );

    let messages = vec![
        CollectdMessage::new("coordinate", "z", 90.0, 0.0),
        CollectdMessage::new("coordinate", "z", 90.0, 1.0),
        CollectdMessage::new("coordinate", "z", 90.0, 2.0),
        CollectdMessage::new("coordinate", "z", 90.0, 3.0),
        CollectdMessage::new("coordinate", "z", 90.0, 4.0),
    ];

    let inputs = vec![
        Input::Tick(fixed_timestamp),
        Input::Message(messages[0].clone()),
        Input::Message(messages[1].clone()),
        Input::Message(messages[2].clone()),
        Input::Message(messages[3].clone()),
        Input::Message(messages[4].clone()),
        Input::Flush,
    ];

    let expected_outputs = vec![
        Output::NextTickAt(fixed_timestamp + one_hour),
        Output::NextTickAt(fixed_timestamp + one_hour),
        Output::NextTickAt(fixed_timestamp + one_hour),
        Output::NextTickAt(fixed_timestamp + one_hour),
        Output::NextTickAt(fixed_timestamp + one_hour),
        Output::MessageBatch(MessageGroup::from_messages(
            vec![messages[0].clone(), messages[1].clone()],
            fixed_timestamp,
        )),
        Output::MessageBatch(MessageGroup::from_messages(
            vec![messages[2].clone(), messages[3].clone()],
            fixed_timestamp,
        )),
        Output::MessageBatch(MessageGroup::from_messages(
            vec![messages[4].clone()],
            fixed_timestamp,
        )),
    ];

    test_batcher(&mut batcher, inputs, expected_outputs);
}

#[test]
fn it_batches_messages_based_on_max_age() {
    use chrono::{prelude::*, Duration};

    let fixed_timestamp = FixedOffset::east(7 * 3600)
        .ymd(2014, 7, 8)
        .and_hms(9, 10, 0);

    let ten_seconds = Duration::seconds(10);

    let mut batcher = MessageBatcher::new(
        Box::new(filter::NoMessageFilter::new()),
        group::MessageGrouper::new(
            Box::new(CollectdTimestampDeltaCriterion { delta: 10000000.0 }),
            Box::new(RetirementPolicy {
                max_group_age: ten_seconds,
            }),
        ),
        ten_seconds,
        fixed_timestamp,
    );

    let messages = vec![
        CollectdMessage::new("coordinate", "z", 90.0, 0.0),
        CollectdMessage::new("coordinate", "z", 90.0, 1.0),
        CollectdMessage::new("coordinate", "z", 90.0, 2.0),
        CollectdMessage::new("coordinate", "z", 90.0, 3.0),
        CollectdMessage::new("coordinate", "z", 90.0, 4.0),
        CollectdMessage::new("coordinate", "z", 90.0, 5.0),
    ];

    macro_rules! assert_handle {
        ($input:expr => [ $($output:expr),* ]) => {
            assert_eq!(batcher.handle_vec($input), vec![$($output),*]);
        };
    }

    assert_handle! {
        Input::Tick(fixed_timestamp) => []
    };
    assert_handle! {
        Input::Message(messages[0].clone()) => [
            Output::NextTickAt(fixed_timestamp + ten_seconds)
        ]
    };
    assert_handle! {
        Input::Message(messages[1].clone()) => [
            Output::NextTickAt(fixed_timestamp + ten_seconds)
        ]
    };
    assert_handle! {
        Input::Tick(fixed_timestamp + Duration::seconds(9)) => [
            Output::NextTickAt(fixed_timestamp + ten_seconds)
        ]
    };
    assert_handle! {
        Input::Message(messages[2].clone()) => [
            Output::NextTickAt(fixed_timestamp + ten_seconds)
        ]
    };
    assert_handle! {
        Input::Tick(fixed_timestamp + Duration::seconds(11)) => [
            Output::MessageBatch(MessageGroup::from_messages(
                vec![
                    messages[0].clone(),
                    messages[1].clone(),
                    messages[2].clone(),
                ],
                fixed_timestamp,
            ))
        ]
    };
    assert_handle! {
        Input::Message(messages[3].clone()) => [
            Output::NextTickAt(fixed_timestamp + Duration::seconds(11) + ten_seconds)
        ]
    };

    assert_handle! {
        Input::Tick(fixed_timestamp + Duration::milliseconds(20999)) => [
            Output::NextTickAt(fixed_timestamp + Duration::seconds(11) + ten_seconds)
        ]
    };

    assert_handle! {
        Input::Message(messages[4].clone()) => [
            Output::NextTickAt(fixed_timestamp + Duration::seconds(11) + ten_seconds)
        ]
    };

    assert_handle! {
         Input::Tick(fixed_timestamp + Duration::seconds(21)) => [
            Output::MessageBatch(MessageGroup::from_messages(
                vec![
                    messages[3].clone(),
                    messages[4].clone(),
                ],
                fixed_timestamp + Duration::seconds(11),
            ))

        ]
    };

    assert_handle! {
        Input::Message(messages[5].clone()) => [
            Output::NextTickAt(fixed_timestamp + Duration::seconds(21) + ten_seconds)
        ]
    };

    assert_handle! {
         Input::Flush => [
            Output::MessageBatch(MessageGroup::from_messages(
                vec![
                    messages[5].clone(),
                ],
                fixed_timestamp + Duration::seconds(21),
            ))

        ]
    };
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
