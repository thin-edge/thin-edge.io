use crate::{address::AnyMessageBox, plugin::Message};

/// A message that can contain any other message
///
/// This is solely used in conjunction with [`AnyMessages`](crate::plugin::AnyMessages) and should not generally be used
/// otherwise.
///
/// To construct it, you will need to have a message and call [`AnyMessage::from_message`]
#[derive(Debug)]
pub struct AnyMessage(pub(crate) AnyMessageBox);

impl std::ops::Deref for AnyMessage {
    type Target = dyn Message;

    fn deref(&self) -> &Self::Target {
        &*self.0
    }
}

impl AnyMessage {
    /// Construct a new [`AnyMessage`] from a message
    pub fn from_message<M: Message>(m: M) -> Self {
        AnyMessage(Box::new(m))
    }

    /// Try to downcast this message to a specific message
    pub fn downcast<M: Message>(self) -> Result<M, Self> {
        Ok(*self.0.downcast().map_err(AnyMessage)?)
    }

    /// Take out the raw boxed message
    ///
    /// Note
    ///
    /// This is an advanced API and should only be used if needed.
    /// Prefer using `AnyMessage::downcast` if possible
    pub fn into_raw(self) -> AnyMessageBox {
        self.0
    }
}

impl Message for AnyMessage {}

/// The type of a message as used by `tedge_api` to represent a type
#[derive(Debug, Clone)]
pub struct MessageType {
    name: &'static str,
    kind: MessageKind,
}

#[derive(Debug, Clone)]
enum MessageKind {
    Wildcard,
    Typed(std::any::TypeId),
}

impl MessageType {
    /// Does this [`MessageType`] satisfy another [`MessageType`]
    ///
    /// ## Note
    /// A message type from [`AnyMessage`] acts as a 'wildcard', being satisfied by any other type
    /// (even itself).
    /// The reverse is not true, a specific type cannot be satisfied by a 'wildcard' (i.e.
    /// [`AnyMessage`]).
    ///
    /// [`MessageType::satisfy`] is thus reflexive but not symmetric nor transitive, meaning that it cannot be
    /// used for `PartialEq`.
    #[must_use]
    pub fn satisfy(&self, other: &Self) -> bool {
        match (&self.kind, &other.kind) {
            (MessageKind::Wildcard, _) => true,
            (_, MessageKind::Wildcard) => false,
            (MessageKind::Typed(ty_l), MessageKind::Typed(ty_r)) => ty_l.eq(ty_r),
        }
    }

    /// Get the [`MessageType`] for a `M`:[`Message`]
    #[must_use]
    pub fn for_message<M: Message>() -> Self {
        let id = std::any::TypeId::of::<M>();
        MessageType {
            name: std::any::type_name::<M>(),
            kind: if id == std::any::TypeId::of::<AnyMessage>() {
                MessageKind::Wildcard
            } else {
                MessageKind::Typed(id)
            },
        }
    }

    pub(crate) fn from_message(msg: &dyn Message) -> Self {
        let id = msg.type_id();
        MessageType {
            name: msg.type_name(),
            kind: if id == std::any::TypeId::of::<AnyMessage>() {
                MessageKind::Wildcard
            } else {
                MessageKind::Typed(id)
            },
        }
    }

    /// Get the type's name
    #[must_use]
    pub fn name(&self) -> &str {
        self.name
    }
}

/// A message to tell the core to stop thin-edge
#[derive(Debug)]
pub struct StopCore;

impl Message for StopCore {}

crate::make_receiver_bundle!(pub struct CoreMessages(StopCore));

#[cfg(test)]
mod tests {
    use crate::Message;

    use super::{AnyMessage, MessageType};

    #[derive(Debug)]
    struct Bar;

    impl Message for Bar {}

    #[derive(Debug)]
    struct Foo;

    impl Message for Foo {}

    #[test]
    fn assert_satisfy_laws_for_types() {
        let bar_type = MessageType::for_message::<Bar>();
        let any_message_type = MessageType::for_message::<AnyMessage>();
        let foo_type = MessageType::for_message::<Foo>();

        assert!(any_message_type.satisfy(&bar_type));
        assert!(any_message_type.satisfy(&foo_type));

        assert!(!bar_type.satisfy(&any_message_type));
        assert!(!bar_type.satisfy(&foo_type));
    }
}
