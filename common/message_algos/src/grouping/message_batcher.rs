use crate::{grouping::*, Envelope, Message, Timestamp};

/// A concrete implementation of a `MessageGrouper`.
pub struct MessageBatcher<T: Message> {
    grouping_policy: Box<dyn GroupingPolicy<Message = T>>,
    retirement_policy: Box<dyn RetirementPolicy<Message = T>>,
    groups: Vec<MessageGroup<T>>,
}

impl<T: Message> MessageGrouper for MessageBatcher<T> {
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
    fn retire_groups(&mut self, now: Timestamp) -> RetireGroupsAction<Self::Message> {
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

        RetireGroupsAction {
            retired_groups,
            next_check_at,
        }
    }

    fn retire_groups_unconditionally(&mut self) -> RetireGroupsAction<Self::Message> {
        RetireGroupsAction {
            retired_groups: std::mem::replace(&mut self.groups, Vec::new()),
            next_check_at: None,
        }
    }
}

impl<T: Message> MessageBatcher<T> {
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
