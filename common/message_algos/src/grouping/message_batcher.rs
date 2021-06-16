use crate::{grouping::*, Envelope, Timestamp};

/// A concrete implementation of a `MessageGrouper`.
pub struct MessageBatcher<T: Send + Clone> {
    grouping_policy: Box<dyn GroupingPolicy<Message = T>>,
    retirement_policy: Box<dyn RetirementPolicy<Message = T>>,
    groups: Vec<MessageGroup<T>>,
}

impl<T: Clone + Send> MessageGrouper for MessageBatcher<T> {
    type Message = T;

    /// Enqueue a message into one of the groups, or create a new message group.
    fn add_message(&mut self, message: Envelope<Self::Message>) {
        for group in self.groups.iter_mut() {
            if self.grouping_policy.belongs_to_group(&message, group) {
                group.add(message);
                return;
            }
        }

        // `message` does not fit into any of the existing groups.
        let new_group = MessageGroup::new(message);
        self.groups.push(new_group);
    }

    /// Retire groups. The decision is based on the current system time `now` and the retirement
    /// policy.
    fn retire_groups(&mut self, now: Timestamp) -> RetireGroups<Self::Message> {
        let mut retired_groups = Vec::new();
        let mut next_check_at: Option<Timestamp> = None;

        // XXX: Use `Vec::drain_filter` once it becomes stable.
        let mut i = 0;
        while i < self.groups.len() {
            match self
                .retirement_policy
                .check_retirement(&self.groups[i], now)
            {
                RetirementDecision::Retire => {
                    retired_groups.push(self.groups.remove(i));
                }
                RetirementDecision::NextCheckAt(timestamp) => {
                    i += 1;
                    next_check_at =
                        Some(next_check_at.map(|t| timestamp.min(t)).unwrap_or(timestamp));
                }
            }
        }

        RetireGroups {
            retired_groups,
            next_check_at,
        }
    }

    /// Flushes all groups.
    fn flush_groups(&mut self) -> Vec<MessageGroup<Self::Message>> {
        std::mem::replace(&mut self.groups, Vec::new())
    }
}

impl<T: Clone + Send> MessageBatcher<T> {
    pub fn new(
        grouping_policy: Box<dyn GroupingPolicy<Message = T>>,
        retirement_policy: Box<dyn RetirementPolicy<Message = T>>,
    ) -> Self {
        Self {
            grouping_policy,
            retirement_policy,
            groups: Vec::new(),
        }
    }
}
