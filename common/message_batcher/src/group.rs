//! Message grouping

use crate::Timestamp;

/// Test whether `msg` is a member of a group.
pub trait IsGroupMember<T: Send>: Send {
    fn is_group_member(&self, msg: &T, group: &MessageGroup<T>) -> bool;
}

/// Decision on whether a message group can be retired or not.
pub trait CanRetire<T: Send>: Send {
    fn can_retire(&self, group: &MessageGroup<T>, now: Timestamp) -> bool;
}

/// A group of messages. Guaranteed to contain at least one message.
#[derive(Debug, PartialEq)]
pub struct MessageGroup<T: Send> {
    /// Creation date of the group.
    created_at: Timestamp,
    messages: Vec<T>,
}

impl<T: Send> MessageGroup<T> {
    pub fn new(first_message: T, created_at: Timestamp) -> Self {
        Self {
            created_at,
            messages: vec![first_message],
        }
    }

    pub fn from_messages(messages: Vec<T>, created_at: Timestamp) -> Self {
        assert!(messages.len() > 0);
        Self {
            created_at,
            messages,
        }
    }

    pub fn iter_messages(&self) -> impl Iterator<Item = &T> {
        self.messages.iter()
    }

    pub fn first(&self) -> &T {
        &self.messages[0]
    }

    pub fn add(&mut self, message: T) {
        self.messages.push(message);
    }

    pub fn created_at(&self) -> Timestamp {
        self.created_at
    }
}

pub struct MessageGrouper<T: Send> {
    group_cond: Box<dyn IsGroupMember<T>>,
    retire_cond: Box<dyn CanRetire<T>>,
    groups: Vec<MessageGroup<T>>,
}

impl<T: Clone + Send> MessageGrouper<T> {
    pub fn new(group_cond: Box<dyn IsGroupMember<T>>, retire_cond: Box<dyn CanRetire<T>>) -> Self {
        Self {
            group_cond,
            retire_cond,
            groups: Vec::new(),
        }
    }

    /// Enqueue a message into one of the groups, or create a new message group.
    pub fn group_message(&mut self, message: T, received_at: Timestamp) {
        for group in self.groups.iter_mut() {
            if self.group_cond.is_group_member(&message, group) {
                group.add(message);
                return;
            }
        }

        // `message` does not fit into any of the existing groups.
        let new_group = MessageGroup::new(message, received_at);
        self.groups.push(new_group);
    }

    pub fn min_created_at(&self) -> Option<Timestamp> {
        self.groups.iter().map(|group| group.created_at).min()
    }

    /// Retiring of groups is based on `current_timestamp` (and group size)
    pub fn retire_groups(&mut self, current_timestamp: Timestamp) -> Vec<MessageGroup<T>> {
        let mut retired_groups = Vec::new();

        // XXX: Use `Vec::drain_filter` once it becomes stable.
        let mut i = 0;
        while i < self.groups.len() {
            if self
                .retire_cond
                .can_retire(&self.groups[i], current_timestamp)
            {
                retired_groups.push(self.groups.remove(i));
            } else {
                i += 1;
            }
        }
        retired_groups
    }

    /// Flushes all groups.
    pub fn flush_groups(&mut self) -> Vec<MessageGroup<T>> {
        std::mem::replace(&mut self.groups, Vec::new())
    }
}
