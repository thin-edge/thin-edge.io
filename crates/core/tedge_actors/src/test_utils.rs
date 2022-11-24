use crate::{ChannelError, Message, Recipient, Sender};
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::Mutex;

/// A recipient that collects all the messages in a vector
///
/// Mostly useful for testing
#[derive(Clone, Debug)]
pub struct VecRecipient<M> {
    messages: Arc<Mutex<Vec<M>>>,
}

#[async_trait]
impl<M: Message> Sender<M> for VecRecipient<M> {
    async fn send(&mut self, message: M) -> Result<(), ChannelError> {
        let mut messages = self.messages.lock().await;
        messages.push(message);
        Ok(())
    }

    fn recipient_clone(&self) -> Recipient<M> {
        Box::new(VecRecipient {
            messages: self.messages.clone(),
        })
    }
}

impl<M: Message> Default for VecRecipient<M> {
    fn default() -> Self {
        VecRecipient {
            messages: Arc::new(Mutex::new(vec![])),
        }
    }
}

impl<M: Message + Clone> VecRecipient<M> {
    pub fn as_recipient(&self) -> Recipient<M> {
        Box::new(self.clone())
    }

    pub async fn collect(&self) -> Vec<M> {
        let messages = self.messages.lock().await;
        messages.clone()
    }
}
