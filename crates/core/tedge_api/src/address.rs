use std::{marker::PhantomData, time::Duration};

use crate::plugin::Message;

#[doc(hidden)]
pub type AnySendBox = Box<dyn std::any::Any + Send>;

#[doc(hidden)]
#[derive(Debug)]
pub struct InternalMessage {
    pub(crate) data: AnySendBox,
    pub(crate) reply_sender: tokio::sync::oneshot::Sender<AnySendBox>,
}

/// THIS IS NOT PART OF THE PUBLIC API, AND MAY CHANGE AT ANY TIME
#[doc(hidden)]
pub type MessageSender = tokio::sync::mpsc::Sender<InternalMessage>;

/// THIS IS NOT PART OF THE PUBLIC API, AND MAY CHANGE AT ANY TIME
#[doc(hidden)]
pub type MessageReceiver = tokio::sync::mpsc::Receiver<InternalMessage>;

/// An address of a plugin that can receive messages a certain type of messages
///
/// An instance of this type represents an address that can be used to send messages of a
/// well-defined type to a specific plugin.
/// The `Address` instance can be used to send messages of several types, but each type has to be
/// in `MB: MessageBundle`.
pub struct Address<MB: ReceiverBundle> {
    _pd: PhantomData<fn(MB)>,
    sender: MessageSender,
}

impl<MB: ReceiverBundle> std::fmt::Debug for Address<MB> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct(&format!("Address<{}>", std::any::type_name::<MB>()))
            .finish_non_exhaustive()
    }
}

impl<MB: ReceiverBundle> Clone for Address<MB> {
    fn clone(&self) -> Self {
        Self {
            _pd: PhantomData,
            sender: self.sender.clone(),
        }
    }
}

impl<MB: ReceiverBundle> Address<MB> {
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
    pub async fn send<M: Message>(&self, msg: M) -> Result<ReplyReceiver<M::Reply>, M>
    where
        MB: Contains<M>,
    {
        let (sender, receiver) = tokio::sync::oneshot::channel();

        self.sender
            .send(InternalMessage {
                data: Box::new(msg),
                reply_sender: sender,
            })
            .await
            .map_err(|msg| *msg.0.data.downcast::<M>().unwrap())?;

        Ok(ReplyReceiver {
            _pd: PhantomData,
            reply_recv: receiver,
        })
    }
}

#[derive(Debug)]
pub struct ReplyReceiver<M> {
    _pd: PhantomData<fn(M)>,
    reply_recv: tokio::sync::oneshot::Receiver<AnySendBox>,
}

impl<M: Message> ReplyReceiver<M> {
    pub async fn wait_for_reply(self, timeout: Duration) -> Result<M, ReplyError> {
        let data = tokio::time::timeout(timeout, self.reply_recv)
            .await
            .map_err(|_| ReplyError::Timeout)?
            .map_err(|_| ReplyError::Unknown)?;

        Ok(*data.downcast().expect("Invalid type received"))
    }
}

#[derive(Debug)]
pub struct ReplySender<M> {
    _pd: PhantomData<fn(M)>,
    reply_sender: tokio::sync::oneshot::Sender<AnySendBox>,
}

impl<M: Message> ReplySender<M> {
    pub(crate) fn new(reply_sender: tokio::sync::oneshot::Sender<AnySendBox>) -> Self {
        Self {
            _pd: PhantomData,
            reply_sender,
        }
    }

    pub fn reply(self, msg: M) -> Result<(), M> {
        self.reply_sender
            .send(Box::new(msg))
            .map_err(|msg| *msg.downcast::<M>().unwrap())
    }

    /// Check whether the ReplySender is closed
    ///
    /// This function returns when the internal communication channel is closed.
    /// This can be used (with e.g. [tokio::select]) to check whether the message sender stopped
    /// waiting for a reply.
    pub async fn closed(&mut self) {
        self.reply_sender.closed().await
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ReplyError {
    #[error("There was no response before timeout")]
    Timeout,
    #[error("Could not send reply")]
    Unknown,
}

#[doc(hidden)]
pub trait ReceiverBundle: Send + 'static {
    fn get_ids() -> Vec<(&'static str, std::any::TypeId)>;
}

#[doc(hidden)]
pub trait Contains<M: Message> {}

/// Declare a set of messages to be a "MessageBundle"
///
/// This macro can be used by a plugin author to declare a set of messages to be a `MessageBundle`.
#[macro_export]
macro_rules! make_receiver_bundle {
    ($pu:vis struct $name:ident($($msg:ty),+)) => {
        #[allow(missing_docs)]
        #[derive(Debug)]
        $pu struct $name;

        impl $crate::address::ReceiverBundle for $name {
            #[allow(unused_parens)]
            fn get_ids() -> Vec<(&'static str, std::any::TypeId)> {
                vec![
                    $((std::any::type_name::<$msg>(), std::any::TypeId::of::<$msg>())),+
                ]
            }
        }

        $(impl $crate::address::Contains<$msg> for $name {})+
    };
}

#[cfg(test)]
mod tests {
    use static_assertions::{assert_impl_all, assert_not_impl_any};

    use crate::{
        address::{ReplyReceiver, ReplySender},
        make_receiver_bundle,
        plugin::Message,
        Address,
    };

    #[derive(Debug)]
    struct Foo;

    impl Message for Foo {
        type Reply = Bar;
    }

    #[derive(Debug)]
    struct Bar;

    impl Message for Bar {
        type Reply = Bar;
    }

    make_receiver_bundle!(struct FooBar(Foo, Bar));

    #[allow(unreachable_code, dead_code, unused)]
    fn check_compile() {
        let addr: Address<FooBar> = todo!();
        addr.send(Foo);
        addr.send(Bar);
    }

    /////// Assert that types have the correct traits

    #[allow(dead_code)]
    struct NotSync {
        _pd: std::marker::PhantomData<*const ()>,
    }

    assert_impl_all!(Address<FooBar>: Clone, Send, Sync);

    assert_not_impl_any!(NotSync: Send, Sync);
    assert_impl_all!(ReplySender<NotSync>: Send, Sync);
    assert_impl_all!(ReplyReceiver<NotSync>: Send, Sync);
}
