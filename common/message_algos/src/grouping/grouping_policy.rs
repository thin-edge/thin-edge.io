use crate::{grouping::MessageGroup, Envelope};

/// The policy to add a message to an existing group.
///
/// If a message doesn't fall into any of the existing groups,
/// we start a new group.
pub trait GroupingPolicy: Send {
    type Message: Send + Clone;

    fn belongs_to_group(
        &self,
        _message: &Envelope<Self::Message>,
        group: &MessageGroup<Self::Message>,
    ) -> bool;
}
