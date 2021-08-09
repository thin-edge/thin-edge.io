use mqtt_client::Topic;

#[derive(Debug, Clone)]
pub(crate) enum SoftwareCommand {
    SoftwareList,
    SoftwareUpdate,
    Cumulocity,
    UnknownOperation,
}

impl From<Topic> for SoftwareCommand {
    fn from(topic: Topic) -> Self {
        match topic.name.as_str() {
            r#"tedge/commands/res/software/list"# => Self::SoftwareList,
            r#"tedge/commands/res/software/update"# => Self::SoftwareUpdate,
            r#"c8y/s/ds"# => Self::Cumulocity,
            _ => Self::UnknownOperation,
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
            list: Topic::new("tedge/commands/req/software/list").expect("Invalid topic"),
            update: Topic::new("tedge/commands/req/software/update").expect("Invalid topic"),
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
