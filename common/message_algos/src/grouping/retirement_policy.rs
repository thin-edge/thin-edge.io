use crate::{grouping::MessageGroup, Timestamp};

/// Decision on retiring message groups ("batches").
pub trait RetirementPolicy: Send {
    type Message: Send + Clone;

    /// Decision whether to retire a group based on the current system time.
    fn check_retirement(
        &self,
        group: &MessageGroup<Self::Message>,
        now: Timestamp,
    ) -> RetirementDecision;
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum RetirementDecision {
    /// Retire the group now.
    Retire,
    /// Check for retirement at a later point in time.
    NextCheckAt(Timestamp),
}
