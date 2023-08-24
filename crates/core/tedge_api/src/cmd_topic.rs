use crate::entity::EntityTopic;
use crate::entity_store::EntityMetadata;
use mqtt_channel::Topic;
use mqtt_channel::TopicFilter;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Target {
    entity: EntityMetadata,
    cmd_id: String,
}

impl Target {
    pub fn new(entity: &EntityMetadata, cmd_id: &str) -> Self {
        Target {
            entity: entity.clone(),
            cmd_id: cmd_id.to_string(),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum CmdPublishTopic {
    // Restart(Target),
    // SoftwareList(Target),
    // SoftwareUpdate(Target),
    // ConfigSnapshot(Target),
    // ConfigUpdate(Target),
    LogUpload(Target),
}

impl CmdPublishTopic {
    pub fn to_topic(&self, prefix: &str) -> Topic {
        let topic = match self {
            CmdPublishTopic::LogUpload(target) => {
                format!(
                    "{}/{}/cmd/log_upload/{}",
                    prefix, target.entity.topic_id, target.cmd_id
                )
            }
        };
        Topic::new_unchecked(&topic)
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum CmdSubscribeTopic {
    // Restart,
    // SoftwareList,
    // SoftwareUpdate,
    // ConfigSnapshot,
    // ConfigUpdate,
    LogUpload,
}

impl CmdSubscribeTopic {
    pub fn metadata(&self, prefix: &str) -> String {
        match self {
            CmdSubscribeTopic::LogUpload => format!("{prefix}/device/+///cmd/log_upload"),
        }
    }

    pub fn with_id(&self, prefix: &str) -> String {
        format!("{}/+", self.metadata(prefix))
    }
}

/// Return device topic ID and command ID
pub fn get_target_ids_from_cmd_topic(
    topic: &Topic,
    prefix: &str,
) -> (Option<String>, Option<String>) {
    let mut cmd_topic_filter = TopicFilter::new_unchecked(&format!("{prefix}/+/+/+/+/cmd/+/+"));
    cmd_topic_filter.add_unchecked(&format!("{prefix}/+/+/+/+/cmd/+"));

    if cmd_topic_filter.accept_topic(topic) {
        let entity_topic: Option<EntityTopic> = topic.try_into().ok();
        match entity_topic {
            Some(entity_topic) => {
                let entity_topic_id = entity_topic.entity_id().to_string();
                match entity_topic.channel() {
                    Some(channel) => {
                        let maybe_cmd_id = if channel.suffix.is_empty() {
                            None
                        } else {
                            Some(channel.suffix.to_string())
                        };
                        (Some(entity_topic_id), maybe_cmd_id)
                    }
                    None => (None, None),
                }
            }
            None => (None, None),
        }
    } else {
        (None, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_case::test_case;

    #[test_case("te/device/main///cmd/log_upload/abcd", (Some("device/main//".into()), Some("abcd".into())); "valid main device and cmd id")]
    #[test_case("te/device/child///cmd/log_upload/abcd", (Some("device/child//".into()), Some("abcd".into())); "valid child device and cmd id")]
    #[test_case("te/device/main///cmd/log_upload", (Some("device/main//".into()), None); "metadata topic for main")]
    #[test_case("te/device/child///cmd/log_upload", (Some("device/child//".into()), None); "metadata topic for child")]
    #[test_case("te/device////cmd/log_upload/", (Some("device///".into()), None); "both device and cmd id are missing")]
    #[test_case("foo/bar", (None, None); "invalid topic")]
    fn extract_ids_from_cmd_topic(topic: &str, expected_pair: (Option<String>, Option<String>)) {
        let topic = Topic::new_unchecked(topic);
        assert_eq!(get_target_ids_from_cmd_topic(&topic, "te"), expected_pair);
    }
}
