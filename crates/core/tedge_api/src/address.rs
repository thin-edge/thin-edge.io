use std::marker::PhantomData;

use crate::plugin::{Contains, Message, MessageBundle};

/// THIS IS NOT PART OF THE PUBLIC API, AND MAY CHANGE AT ANY TIME
#[doc(hidden)]
pub type MessageSender = tokio::sync::mpsc::Sender<Box<dyn std::any::Any + Send>>;

/// THIS IS NOT PART OF THE PUBLIC API, AND MAY CHANGE AT ANY TIME
#[doc(hidden)]
pub type MessageReceiver = tokio::sync::mpsc::Receiver<Box<dyn std::any::Any + Send>>;

/// An address of a plugin that can receive messages a certain type of messages
///
/// An instance of this type represents an address that can be used to send messages of a
/// well-defined type to a specific plugin.
/// The `Address` instance can be used to send messages of several types, but each type has to be
/// in `MB: MessageBundle`.
pub struct Address<MB: MessageBundle> {
    _pd: PhantomData<MB>,
    sender: MessageSender,
}

impl<MB: MessageBundle> std::fmt::Debug for Address<MB> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct(&format!("Address<{}>", std::any::type_name::<MB>()))
            .finish_non_exhaustive()
    }
}

impl<MB: MessageBundle> Clone for Address<MB> {
    fn clone(&self) -> Self {
        Self {
            _pd: PhantomData,
            sender: self.sender.clone(),
        }
    }
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

    /// Send a message `M` to the address represented by the instance of this struct
    ///
    /// This function can be used to send a message of type `M` to the plugin that is addressed by
    /// the instance of this type.
    ///
    /// # Return
    ///
    /// The function either returns `Ok(())` if sending the message succeeded,
    /// or the message in the error variant of the `Result`: `Err(M)`.
    ///
    /// The error is returned if the receiving side (the plugin that is addressed) does not receive
    /// messages anymore.
    ///
    /// # Details
    ///
    /// For details on sending and receiving, see `tokio::sync::mpsc::Sender`.
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
