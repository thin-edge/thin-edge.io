use std::time::Duration;
use tedge_actors::Message;

/// Request a timeout to be set
///
/// After the given duration the timer will return the event back to the caller.
#[derive(Debug)]
pub struct SetTimeout<T: Message> {
    pub duration: Duration,
    pub event: T,
}

/// Timeout event sent by the timer back to the caller
#[derive(Debug)]
pub struct Timeout<T: Message> {
    pub event: T,
}
