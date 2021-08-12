use json_sm::{SoftwareListRequest, SoftwareUpdateRequest};
use mqtt_client::Topic;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum SoftwareCommand {
    SoftwareList,
    SoftwareUpdate,
    Cumulocity,
    UnknownCommand,
}

impl From<Topic> for SoftwareCommand {
    fn from(topic: Topic) -> Self {
        match topic.name.as_str() {
            r#"tedge/commands/res/software/list"# => Self::SoftwareList,
            r#"tedge/commands/res/software/update"# => Self::SoftwareUpdate,
            r#"c8y/s/ds"# => Self::Cumulocity,
            _ => Self::UnknownCommand,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct SoftwareTopics {
    pub publish: SoftwarePublishTopics,
    pub subscribe: SoftwareSubscribeTopics,
}

#[derive(Debug, Clone)]
pub(crate) struct SoftwarePublishTopics {
    pub list: Topic,
    pub update: Topic,
    pub c8y: Topic,
}

#[derive(Debug, Clone)]
pub(crate) struct SoftwareSubscribeTopics {
    pub software: &'static str,
    pub c8y: &'static str,
}

impl Default for SoftwareTopics {
    fn default() -> Self {
        Self {
            publish: Default::default(),
            subscribe: Default::default(),
        }
    }
}

impl Default for SoftwarePublishTopics {
    fn default() -> Self {
        Self {
            list: Topic::new(SoftwareListRequest::topic_name()).expect("Invalid topic"),
            update: Topic::new(SoftwareUpdateRequest::topic_name()).expect("Invalid topic"),
            c8y: Topic::new("c8y/s/us").expect("Invalid topic"),
        }
    }
}

impl Default for SoftwareSubscribeTopics {
    fn default() -> Self {
        Self {
            software: "tedge/commands/res/software/#",
            c8y: "c8y/s/ds",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn convert_software_command() {
        let sw_list_command: SoftwareCommand = Topic::new("tedge/commands/res/software/list")
            .unwrap()
            .into();
        assert_eq!(sw_list_command, SoftwareCommand::SoftwareList);
        let sw_update_command: SoftwareCommand = Topic::new("tedge/commands/res/software/update")
            .unwrap()
            .into();
        assert_eq!(sw_update_command, SoftwareCommand::SoftwareUpdate);
        let c8y_command: SoftwareCommand = Topic::new("c8y/s/ds").unwrap().into();
        assert_eq!(c8y_command, SoftwareCommand::Cumulocity);
        let unknown_command: SoftwareCommand = Topic::new("unknown").unwrap().into();
        assert_eq!(unknown_command, SoftwareCommand::UnknownCommand);
    }

    #[test]
    fn validate_topics() {
        let topics = SoftwareTopics::default();
        assert_eq!(topics.publish.list.name, "tedge/commands/req/software/list");
        assert_eq!(
            topics.publish.update.name,
            "tedge/commands/req/software/update"
        );
        assert_eq!(topics.publish.c8y.name, "c8y/s/us");
        assert_eq!(topics.subscribe.software, "tedge/commands/res/software/#");
        assert_eq!(topics.subscribe.c8y, "c8y/s/ds");
    }
}
