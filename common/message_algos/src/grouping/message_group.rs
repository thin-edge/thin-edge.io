use crate::Envelope;

/// A group of messages (or message envelopes). Guaranteed to contain at least one message.
#[derive(Debug)]
pub struct MessageGroup<T: Send + Clone> {
    messages: Vec<Envelope<T>>,
}

impl<T: Send + Clone> MessageGroup<T> {
    pub fn new(first_message: Envelope<T>) -> Self {
        Self {
            messages: vec![first_message],
        }
    }

    pub fn from_messages(messages: Vec<Envelope<T>>) -> Self {
        assert!(messages.len() > 0);
        Self { messages }
    }

    pub fn iter_messages(&self) -> impl Iterator<Item = &Envelope<T>> {
        self.messages.iter()
    }

    pub fn first(&self) -> &Envelope<T> {
        &self.messages[0]
    }

    pub fn add(&mut self, message: Envelope<T>) {
        self.messages.push(message);
    }
}
