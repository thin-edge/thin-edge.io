use crate::error::TopicError;
use mqtt_channel::Topic;
use mqtt_channel::TopicFilter;
use std::convert::TryFrom;

// TODO! "te" must be configurable
const CMD_TOPIC_FILTER: &str = "te/device/+/+/+/cmd/+/+";

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum DeviceKind {
    Main,
    Child(String),
}

impl DeviceKind {
    pub fn to_string(&self) -> String {
        match self {
            DeviceKind::Main => "main".to_string(),
            DeviceKind::Child(child_id) => child_id.to_string(),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum ResponseTopic {
    SoftwareListResponse,
    SoftwareUpdateResponse,
    RestartResponse,
}

impl ResponseTopic {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SoftwareListResponse => r#"tedge/commands/res/software/list"#,
            Self::SoftwareUpdateResponse => r#"tedge/commands/res/software/update"#,
            Self::RestartResponse => r#"tedge/commands/res/control/restart"#,
        }
    }
}

impl TryFrom<String> for ResponseTopic {
    type Error = TopicError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.as_str() {
            r#"tedge/commands/res/software/list"# => Ok(ResponseTopic::SoftwareListResponse),
            r#"tedge/commands/res/software/update"# => Ok(ResponseTopic::SoftwareUpdateResponse),
            r#"tedge/commands/res/control/restart"# => Ok(ResponseTopic::RestartResponse),
            err => Err(TopicError::UnknownTopic {
                topic: err.to_string(),
            }),
        }
    }
}

impl TryFrom<&str> for ResponseTopic {
    type Error = TopicError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::try_from(value.to_string())
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum RequestTopic {
    SoftwareListRequest,
    SoftwareUpdateRequest,
    RestartRequest,
}

impl RequestTopic {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SoftwareListRequest => r#"tedge/commands/req/software/list"#,
            Self::SoftwareUpdateRequest => r#"tedge/commands/req/software/update"#,
            Self::RestartRequest => r#"tedge/commands/req/control/restart"#,
        }
    }
}

pub fn get_child_id_from_child_topic(topic: &str) -> Option<String> {
    let mut topic_split = topic.split('/');
    // the second element is the child id
    topic_split.nth(1).and_then(|id| {
        if id.is_empty() {
            None
        } else {
            Some(id.to_string())
        }
    })
}

pub fn get_target_ids_from_cmd_topic(topic: &Topic) -> Option<(DeviceKind, String)> {
    let cmd_topic_filter: TopicFilter = CMD_TOPIC_FILTER.try_into().unwrap();

    if cmd_topic_filter.accept_topic(topic) {
        // with the topic scheme te/device/<device-id>///cmd/<cmd-name>/<cmd-id>

        let mut topic_split = topic.name.split('/');
        // the 3rd level is the device id
        let maybe_device_id = topic_split.nth(2).filter(|s| !s.is_empty());
        // the last element is the command id
        let maybe_cmd_id = topic_split.last().filter(|s| !s.is_empty());

        match (maybe_device_id, maybe_cmd_id) {
            (Some(device_id), Some(cmd_id)) => {
                if device_id == "main" {
                    Some((DeviceKind::Main, cmd_id.into()))
                } else {
                    Some((DeviceKind::Child(device_id.into()), cmd_id.into()))
                }
            }
            _ => None,
        }
    } else {
        None
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

impl From<CmdPublishTopic> for Topic {
    fn from(value: CmdPublishTopic) -> Self {
        let topic = match value {
            CmdPublishTopic::LogUpload(target) => {
                format!(
                    "te/device/{}///cmd/log_upload/{}",
                    target.device_id, target.cmd_id
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
    LogUpload,
    // ConfigSnapshot,
    // ConfigUpdate,
}

impl From<CmdSubscribeTopic> for &str {
    fn from(value: CmdSubscribeTopic) -> Self {
        match value {
            CmdSubscribeTopic::LogUpload => "te/device/+///cmd/log_upload/+",
        }
    }
}

impl From<CmdSubscribeTopic> for TopicFilter {
    fn from(value: CmdSubscribeTopic) -> Self {
        TopicFilter::new_unchecked(value.into())
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Target {
    device_id: String,
    cmd_id: String,
}

impl Target {
    pub fn new(device_id: String, cmd_id: String) -> Self {
        Target { device_id, cmd_id }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::convert::TryInto;
    use test_case::test_case;

    #[test]
    fn convert_response_topic_to_str() {
        assert_eq!(
            ResponseTopic::SoftwareListResponse.as_str(),
            "tedge/commands/res/software/list"
        );
        assert_eq!(
            ResponseTopic::SoftwareUpdateResponse.as_str(),
            "tedge/commands/res/software/update"
        );
    }

    #[test]
    fn convert_str_into_response_topic() {
        let list: ResponseTopic = "tedge/commands/res/software/list".try_into().unwrap();
        assert_eq!(list, ResponseTopic::SoftwareListResponse);
        let update: ResponseTopic = "tedge/commands/res/software/update".try_into().unwrap();
        assert_eq!(update, ResponseTopic::SoftwareUpdateResponse);

        let error: Result<ResponseTopic, TopicError> = "test".try_into();
        assert!(error.is_err());
    }

    #[test]
    fn convert_request_topic_to_str() {
        assert_eq!(
            RequestTopic::SoftwareListRequest.as_str(),
            "tedge/commands/req/software/list"
        );
        assert_eq!(
            RequestTopic::SoftwareUpdateRequest.as_str(),
            "tedge/commands/req/software/update"
        );
    }

    #[test_case("tedge/child1/commands/firmware/update", Some("child1"); "valid child id")]
    #[test_case("tedge/", None; "invalid child id 1")]
    #[test_case("tedge//commands/firmware/update", None; "invalid child id 2")]
    #[test_case("tedge", None; "invalid child id 3")]
    fn extract_child_id(in_topic: &str, expected_child_id: Option<&str>) {
        assert_eq!(
            get_child_id_from_child_topic(in_topic),
            expected_child_id.map(|s| s.to_string())
        );
    }

    #[test_case("te/device/main///cmd/log_upload/1234", Some((DeviceKind::Main, "1234".into())); "valid main device and cmd id")]
    #[test_case("te/device/child///cmd/log_upload/1234", Some((DeviceKind::Child("child".into()), "1234".into())); "valid child device and cmd id")]
    #[test_case("te/device/child///cmd/log_upload/", None; "cmd id is missing")]
    #[test_case("te/device////cmd/log_upload/1234", None; "device id is missing")]
    #[test_case("foo/bar", None; "invalid topic")]
    fn extract_ids_from_cmd_topic(topic: &str, expected_pair: Option<(DeviceKind, String)>) {
        let topic = Topic::new_unchecked(topic);
        assert_eq!(get_target_ids_from_cmd_topic(&topic), expected_pair);
    }
}
