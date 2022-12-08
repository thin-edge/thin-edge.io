use crate::{Address, ChannelError, Message, Recipient, Sender};
use async_trait::async_trait;

/// A recipient that adds a source id on the fly
pub struct KeyedRecipient<K: Message + Clone, M: Message> {
    key: K,
    address: Address<(K, M)>,
}

impl<K: Message + Clone, M: Message> KeyedRecipient<K, M> {
    pub fn new_recipient(key: K, address: Address<(K, M)>) -> Recipient<M> {
        Box::new(KeyedRecipient { key, address })
    }
}

#[async_trait]
impl<K: Message + Clone, M: Message> Sender<M> for KeyedRecipient<K, M> {
    async fn send(&mut self, message: M) -> Result<(), ChannelError> {
        self.address.send((self.key.clone(), message)).await
    }

    fn recipient_clone(&self) -> Recipient<M> {
        Box::new(KeyedRecipient {
            key: self.key.clone(),
            address: self.address.clone(),
        })
    }
}

/// A vector of recipients to which messages are specifically addressed using a source id
pub struct RecipientVec<M: Message> {
    recipients: Vec<Recipient<M>>,
}

impl<M: Message> RecipientVec<M> {
    pub fn new_recipient(recipients: Vec<Recipient<M>>) -> Recipient<(usize, M)> {
        Box::new(RecipientVec { recipients })
    }
}

#[async_trait]
impl<M: Message> Sender<(usize, M)> for RecipientVec<M> {
    async fn send(&mut self, idx_message: (usize, M)) -> Result<(), ChannelError> {
        let (idx, message) = idx_message;
        if let Some(recipient) = self.recipients.get_mut(idx) {
            recipient.send(message).await?;
        }
        Ok(())
    }

    fn recipient_clone(&self) -> Recipient<(usize, M)> {
        let recipients = self
            .recipients
            .iter()
            .map(|r| r.recipient_clone())
            .collect();
        Box::new(RecipientVec { recipients })
    }
}
