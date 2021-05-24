use mapper_converter::mapper::MapperConfig;
use mqtt_client::Topic;

#[derive(Debug)]
pub struct AzureMapperConfig {
    pub in_topic: Topic,
    pub out_topic: Topic,
    pub errors_topic: Topic,
}

impl Default for AzureMapperConfig {
    fn default() -> Self {
        Self {
            in_topic: Topic::new("tedge/measurements").unwrap(),
            out_topic: Topic::new("az/messages/events/").unwrap(),
            errors_topic: Topic::new("tedge/errors").unwrap(),
        }
    }
}

impl From<AzureMapperConfig> for MapperConfig {
    fn from(value: AzureMapperConfig) -> Self {
        MapperConfig {
            in_topic: value.in_topic,
            out_topic: value.out_topic,
            errors_topic: value.errors_topic,
        }
    }
}
