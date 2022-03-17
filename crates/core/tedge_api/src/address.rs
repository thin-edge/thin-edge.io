use std::marker::PhantomData;

use crate::plugin::{Contains, Message, MessageBundle};

/// THIS IS NOT PART OF THE PUBLIC API, AND MAY CHANGE AT ANY TIME
#[doc(hidden)]
pub type MessageSender = tokio::sync::mpsc::Sender<Box<dyn std::any::Any + Send>>;

/// THIS IS NOT PART OF THE PUBLIC API, AND MAY CHANGE AT ANY TIME
#[doc(hidden)]
pub type MessageReceiver = tokio::sync::mpsc::Receiver<Box<dyn std::any::Any + Send>>;

/// An address of a plugin that can receive messages of type `M`
#[derive(Debug, Clone)]
pub struct Address<MB: MessageBundle> {
    _pd: PhantomData<MB>,
    sender: MessageSender,
}

impl<MB: MessageBundle> Address<MB> {
    /// THIS IS NOT PART OF THE PUBLIC API, AND MAY CHANGE AT ANY TIME
    #[doc(hidden)]
    pub fn new(sender: MessageSender) -> Self {
        Self {
            _pd: PhantomData,
            sender,
        }
    }

    pub async fn send<M: Message>(&self, msg: M) -> Result<(), M>
    where
        MB: Contains<M>,
    {
        self.sender
            .send(Box::new(msg))
            .await
            .map_err(|msg| *msg.0.downcast::<M>().unwrap())
    }
}

#[cfg(test)]
mod tests {
    use crate::{make_message_bundle, plugin::Message, Address};

    struct Foo;

    impl Message for Foo {}

    struct Bar;

    impl Message for Bar {}

    make_message_bundle!(struct FooBar(Foo, Bar));

    #[allow(unreachable_code, dead_code, unused)]
    fn check_compile() {
        let addr: Address<FooBar> = todo!();
        addr.send(Foo);
        addr.send(Bar);
    }
}
