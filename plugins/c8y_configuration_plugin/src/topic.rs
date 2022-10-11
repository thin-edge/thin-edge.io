use mqtt_channel::TopicFilter;

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum ConfigOperationResponseTopic {
    SnapshotResponse,
    UpdateResponse,
}

#[allow(clippy::from_over_into)]
// can not implement From since the topic can be anything (`new_unchecked` can be any &str)
impl Into<TopicFilter> for ConfigOperationResponseTopic {
    fn into(self) -> TopicFilter {
        match self {
            ConfigOperationResponseTopic::SnapshotResponse => {
                TopicFilter::new_unchecked("tedge/+/commands/res/config_snapshot")
            }
            ConfigOperationResponseTopic::UpdateResponse => {
                TopicFilter::new_unchecked("tedge/+/commands/res/config_update")
            }
        }
    }
}
