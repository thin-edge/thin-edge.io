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
/// in `RB: ReceiverBundle`.
pub struct Address<RB: ReceiverBundle> {
    _pd: PhantomData<fn(RB)>,
    sender: MessageSender,
}

impl<RB: ReceiverBundle> std::fmt::Debug for Address<RB> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct(&format!("Address<{}>", std::any::type_name::<RB>()))
            .finish_non_exhaustive()
    }
}

impl<RB: ReceiverBundle> Clone for Address<RB> {
    fn clone(&self) -> Self {
        Self {
            _pd: PhantomData,
            sender: self.sender.clone(),
        }
    }
}

impl<RB: ReceiverBundle> Address<RB> {
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
        RB: Contains<M>,
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
/// Listener that allows one to wait for a reply as sent through [`Address::send`]
pub struct ReplyReceiver<M> {
    _pd: PhantomData<fn(M)>,
    reply_recv: tokio::sync::oneshot::Receiver<AnySendBox>,
}

impl<M: Message> ReplyReceiver<M> {
    /// Wait for a reply until for the duration given in `timeout`
    ///
    /// ## Note
    ///
    /// Plugins could not reply for any number of reasons, hence waiting indefinitely on a reply
    /// can cause problems in long-running applications. As such, one needs to specify how long a
    /// reply should take before another action be taken.
    ///
    /// It is also important, that just because a given `M: Message` has a `M::Reply` type set,
    /// that the plugin that a message was sent to does _not_ have to reply with it. It can choose
    /// to not do so.
    pub async fn wait_for_reply(self, timeout: Duration) -> Result<M, ReplyError> {
        let data = tokio::time::timeout(timeout, self.reply_recv)
            .await
            .map_err(|_| ReplyError::Timeout)?
            .map_err(|_| ReplyError::SendSideClosed)?;

        Ok(*data.downcast().expect("Invalid type received"))
    }
}

#[derive(Debug)]
/// Allows the [`Handle`](crate::plugin::Handle) implementation to reply with a given message as
/// specified by the currently handled message.
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

    /// Reply to the originating plugin with the given message
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
/// An error occured while replying
pub enum ReplyError {
    /// The timeout elapsed before the other plugin responded
    #[error("There was no response before timeout")]
    Timeout,
    /// The other plugin dropped its sending side
    ///
    /// This means that there will never be an answer
    #[error("Could not send reply")]
    SendSideClosed,
}

#[doc(hidden)]
pub trait ReceiverBundle: Send + 'static {
    fn get_ids() -> Vec<(&'static str, std::any::TypeId)>;
}

#[doc(hidden)]
pub trait Contains<M: Message> {}

/// Declare a set of messages to be a [`ReceiverBundle`] which is then used with an [`Address`] to
/// specify which kind of messages a given recipient plugin has to support.
///
/// The list of messages MUST be a subset of the messages the plugin behind `Address` supports.
///
/// ## Example
///
/// ```rust
/// # use tedge_api::{Message, make_receiver_bundle, message::NoReply};
///
/// #[derive(Debug)]
/// struct IntMessage(u8);
///
/// impl Message for IntMessage {
///     type Reply = NoReply;
/// }
///
/// #[derive(Debug)]
/// struct StatusMessage(String);
///
/// impl Message for StatusMessage {
///     type Reply = NoReply;
/// }
///
/// make_receiver_bundle!(struct MessageReceiver(IntMessage, StatusMessage));
///
/// // or if you want to export it
///
/// make_receiver_bundle!(pub struct AnotherMessageReceiver(IntMessage, StatusMessage));
/// ```
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
