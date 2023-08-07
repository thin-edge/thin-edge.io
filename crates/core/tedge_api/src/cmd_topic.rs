use mqtt_channel::Topic;
use mqtt_channel::TopicFilter;

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum DeviceKind {
    Main,
    Child(String),
}

impl DeviceKind {
    /// Return the device name, "main" if DeviceKind::Main, otherwise child ID
    pub fn name(&self) -> String {
        self.name_with_default("main")
    }

    /// Return the device name, the given input if DeviceKind::Main, otherwise child ID
    pub fn name_with_default(&self, name: &str) -> String {
        match self {
            DeviceKind::Main => name.to_string(),
            DeviceKind::Child(child_id) => child_id.to_string(),
        }
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
                    "{}/device/{}///cmd/log_upload/{}",
                    prefix, target.device_id, target.cmd_id
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

/// Return device ID and command ID
pub fn get_target_ids_from_cmd_topic(
    topic: &Topic,
    prefix: &str,
) -> (Option<DeviceKind>, Option<String>) {
    let mut cmd_topic_filter =
        TopicFilter::new_unchecked(&format!("{prefix}/device/+/+/+/cmd/+/+"));
    cmd_topic_filter.add_unchecked(&format!("{prefix}/device/+/+/+/cmd/+"));

    if cmd_topic_filter.accept_topic(topic) {
        // with the topic scheme <root>/device/<device-id>///cmd/<cmd-name>[/<cmd-id>]
        let split_topic: Vec<&str> = topic.name.split('/').collect();

        // the 3rd level is the device id
        let maybe_device_id = split_topic.get(2).filter(|s| !s.is_empty());

        // the 7th level is the command id
        let maybe_cmd_id = split_topic
            .get(7)
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty());

        let maybe_device_kind = match maybe_device_id {
            Some(device_id) => {
                if *device_id == "main" {
                    Some(DeviceKind::Main)
                } else {
                    Some(DeviceKind::Child(device_id.to_string()))
                }
            }
            None => None,
        };

        (maybe_device_kind, maybe_cmd_id)
    } else {
        (None, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_case::test_case;

    #[test]
    fn get_device_name_from_device_kind() {
        assert_eq!(DeviceKind::Main.name(), "main".to_string());
        assert_eq!(DeviceKind::Main.name_with_default("abc"), "abc".to_string());
        assert_eq!(DeviceKind::Child("ch".to_string()).name(), "ch".to_string());
        assert_eq!(
            DeviceKind::Child("ch".to_string()).name_with_default("abc"),
            "ch".to_string()
        );
    }

    #[test_case("te/device/main///cmd/log_upload/abcd", (Some(DeviceKind::Main), Some("abcd".into())); "valid main device and cmd id")]
    #[test_case("te/device/child///cmd/log_upload/abcd", (Some(DeviceKind::Child("child".into())), Some("abcd".into())); "valid child device and cmd id")]
    #[test_case("te/device/main///cmd/log_upload", (Some(DeviceKind::Main), None); "metadata topic for main")]
    #[test_case("te/device/child///cmd/log_upload", (Some(DeviceKind::Child("child".into())), None); "metadata topic for child")]
    #[test_case("te/device////cmd/log_upload/", (None, None); "both device and cmd id are missing")]
    #[test_case("foo/bar", (None, None); "invalid topic")]
    fn extract_ids_from_cmd_topic(
        topic: &str,
        expected_pair: (Option<DeviceKind>, Option<String>),
    ) {
        let topic = Topic::new_unchecked(topic);
        assert_eq!(get_target_ids_from_cmd_topic(&topic, "te"), expected_pair);
    }
}
