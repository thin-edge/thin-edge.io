use crate::{mpsc, ChannelError, DynSender, Message, Sender, StreamExt};
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Collect all the messages of the receiver into a vector
///
/// Note that this will block until all the senders are dropped.
pub async fn collect<M>(mut receiver: mpsc::Receiver<M>) -> Vec<M> {
    let mut messages = vec![];
    while let Some(message) = receiver.next().await {
        messages.push(message);
    }
    messages
}

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

    fn sender_clone(&self) -> DynSender<M> {
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
    pub fn as_sender(&self) -> DynSender<M> {
        Box::new(self.clone())
    }

    pub async fn collect(&self) -> Vec<M> {
        let messages = self.messages.lock().await;
        messages.clone()
    }
}
