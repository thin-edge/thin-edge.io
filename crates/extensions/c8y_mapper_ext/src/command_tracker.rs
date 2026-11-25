use crate::actor::CmdId;
use std::collections::HashMap;
use std::collections::HashSet;
use std::time::Duration;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::OperationType;
use tokio::time::Instant;

/// How often the commands are checked for expiry
const EXPIRY_CHECK_INTERVAL: Duration = Duration::from_secs(3600);
/// How long a command received from Cumulocity is remembered if never completed
const ACTIVE_COMMAND_EXPIRY: Duration = Duration::from_secs(12 * 3600);
/// How long a completed command is remembered, to ignore a late duplicate
const COMPLETED_COMMAND_EXPIRY: Duration = Duration::from_secs(3600);

/// Tracks the commands of the mapper, to avoid creating a command twice for the
/// same operation and to avoid requesting a software list while an equivalent
/// request is still pending
pub(crate) struct CommandTracker {
    /// The commands in progress, with the time they were received from Cumulocity,
    /// or `None` for those received as retained messages, which never expire
    active: HashMap<CmdId, Option<Instant>>,
    recently_completed: HashMap<CmdId, Instant>,
    last_expiry_check: Instant,
    /// The software list commands issued by this mapper, until they are cleared
    pending_software_list_requests: HashMap<CmdId, EntityTopicId>,
    /// The targets for which a software list has to be requested, once the pending
    /// requests are known. A capability message can be received before the retained
    /// requests issued before a restart.
    deferred_software_list_requests: HashSet<EntityTopicId>,
}

impl Default for CommandTracker {
    fn default() -> Self {
        CommandTracker {
            active: HashMap::new(),
            recently_completed: HashMap::new(),
            last_expiry_check: Instant::now(),
            pending_software_list_requests: HashMap::new(),
            deferred_software_list_requests: HashSet::new(),
        }
    }
}

impl CommandTracker {
    /// Whether the command is in progress or has recently completed
    pub fn is_known(&self, cmd_id: &str) -> bool {
        self.active.contains_key(cmd_id) || self.recently_completed.contains_key(cmd_id)
    }

    /// Records a command created for an operation received from Cumulocity
    pub fn started(&mut self, cmd_id: CmdId) {
        self.active.insert(cmd_id, Some(Instant::now()));
    }

    /// Records a command of this mapper received as a retained message, possibly
    /// issued before a restart of the mapper
    pub fn resumed(&mut self, cmd_id: &str, operation: &OperationType, target: &EntityTopicId) {
        // Overrides any insertion time, disabling the time-based expiry
        self.active.insert(cmd_id.to_owned(), None);
        if *operation == OperationType::SoftwareList {
            // It has to be cleared before a new request is issued to the same target
            self.pending_software_list_requests
                .insert(cmd_id.to_owned(), target.clone());
        }
    }

    /// Records a command that has been fully processed and cleared
    pub fn completed(&mut self, cmd_id: &str) {
        self.active.remove(cmd_id);
        self.recently_completed
            .insert(cmd_id.to_owned(), Instant::now());
        self.pending_software_list_requests.remove(cmd_id);
    }

    /// Removes the expired commands, at most once per [`EXPIRY_CHECK_INTERVAL`]
    pub fn remove_expired(&mut self) {
        if self.last_expiry_check.elapsed() <= EXPIRY_CHECK_INTERVAL {
            return;
        }
        self.active.retain(|_, started| {
            started.is_none_or(|started| started.elapsed() <= ACTIVE_COMMAND_EXPIRY)
        });
        self.recently_completed
            .retain(|_, completed| completed.elapsed() <= COMPLETED_COMMAND_EXPIRY);
        self.last_expiry_check = Instant::now();
    }

    pub fn software_list_requested(&mut self, cmd_id: CmdId, target: &EntityTopicId) {
        self.pending_software_list_requests
            .insert(cmd_id, target.clone());
    }

    pub fn defer_software_list_request(&mut self, target: &EntityTopicId) {
        self.deferred_software_list_requests.insert(target.clone());
    }

    /// Forgets the software list requests for a target whose agent has just
    /// (re)started. Those pending since before are not reliable anymore: some might
    /// never be completed or cleared, e.g. a cleared request resumed by the agent.
    pub fn reset_software_list_requests(&mut self, target: &EntityTopicId) {
        self.pending_software_list_requests
            .retain(|_, pending_target| pending_target != target);
        self.deferred_software_list_requests.remove(target);
    }

    pub fn has_deferred_software_list_requests(&self) -> bool {
        !self.deferred_software_list_requests.is_empty()
    }

    /// Takes the targets for which a software list has to be requested, skipping
    /// those with a request issued by this mapper still pending
    pub fn take_deferred_software_list_requests(&mut self) -> Vec<EntityTopicId> {
        std::mem::take(&mut self.deferred_software_list_requests)
            .into_iter()
            .filter(|target| match self.pending_software_list_request(target) {
                Some(cmd_id) => {
                    tracing::info!(
                        "Skipping software list request for {target}: command {cmd_id} is still pending"
                    );
                    false
                }
                None => true,
            })
            .collect()
    }

    /// The id of a software list command issued by this mapper to the given target,
    /// if any is still pending, i.e. not cleared yet whatever its status
    fn pending_software_list_request(&self, target: &EntityTopicId) -> Option<&CmdId> {
        self.pending_software_list_requests
            .iter()
            .find(|(_, pending_target)| *pending_target == target)
            .map(|(cmd_id, _)| cmd_id)
    }

    #[cfg(test)]
    pub fn active_count(&self) -> usize {
        self.active.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn completed_commands_are_forgotten_an_hour_after_completion() {
        let mut commands = CommandTracker::default();
        commands.started("c8y-mapper-1".into());
        commands.completed("c8y-mapper-1");

        tokio::time::advance(EXPIRY_CHECK_INTERVAL + Duration::from_secs(1)).await;
        commands.remove_expired();

        assert!(!commands.is_known("c8y-mapper-1"));
    }

    #[tokio::test(start_paused = true)]
    async fn completed_commands_are_not_forgotten_before_an_hour() {
        let mut commands = CommandTracker::default();
        tokio::time::advance(Duration::from_secs(1800)).await;
        commands.started("c8y-mapper-1".into());
        commands.completed("c8y-mapper-1");

        // Past the hourly check, but only 30 minutes after the completion
        tokio::time::advance(Duration::from_secs(1801)).await;
        commands.remove_expired();

        assert!(commands.is_known("c8y-mapper-1"));
    }

    #[tokio::test(start_paused = true)]
    async fn commands_from_cumulocity_expire_but_resumed_commands_do_not() {
        let mut commands = CommandTracker::default();
        let main = EntityTopicId::default_main_device();
        commands.started("c8y-mapper-1".into());
        commands.resumed("c8y-mapper-2", &OperationType::Restart, &main);

        tokio::time::advance(ACTIVE_COMMAND_EXPIRY + Duration::from_secs(1)).await;
        commands.remove_expired();

        assert!(!commands.is_known("c8y-mapper-1"));
        assert!(commands.is_known("c8y-mapper-2"));
    }
}
