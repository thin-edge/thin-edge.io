use crate::mpsc;
use crate::ChannelError;
use crate::DynSender;
use crate::Message;
use crate::Sender;
use async_trait::async_trait;

/// A sender that adds a key to messages on the fly
pub struct KeyedSender<K, M> {
    key: K,
    sender: mpsc::Sender<(K, M)>,
}

impl<K: Clone, M> Clone for KeyedSender<K, M> {
    fn clone(&self) -> Self {
        KeyedSender {
            key: self.key.clone(),
            sender: self.sender.clone(),
        }
    }
}

impl<K: Message + Clone, M: Message> KeyedSender<K, M> {
    pub fn new_sender(key: K, sender: mpsc::Sender<(K, M)>) -> DynSender<M> {
        Box::new(KeyedSender { key, sender })
    }
}

#[async_trait]
impl<K: Message + Clone, M: Message> Sender<M> for KeyedSender<K, M> {
    async fn send(&mut self, message: M) -> Result<(), ChannelError> {
        self.sender.send((self.key.clone(), message)).await
    }
}

/// A vector of senders addressed using a sender id attached to each message
pub struct SenderVec<M> {
    senders: Vec<DynSender<M>>,
}

impl<M: 'static> Clone for SenderVec<M> {
    fn clone(&self) -> Self {
        SenderVec {
            senders: self.senders.clone(),
        }
    }
}

impl<M: Message> SenderVec<M> {
    pub fn new_sender(senders: Vec<DynSender<M>>) -> DynSender<(usize, M)> {
        Box::new(SenderVec { senders })
    }
}

#[async_trait]
impl<M: Message> Sender<(usize, M)> for SenderVec<M> {
    async fn send(&mut self, idx_message: (usize, M)) -> Result<(), ChannelError> {
        let (idx, message) = idx_message;
        if let Some(sender) = self.senders.get_mut(idx) {
            sender.send(message).await?;
        }
        Ok(())
    }
}
