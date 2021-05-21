use crate::mapper::*;
use mqtt_client::Topic;

#[derive(Debug)]
pub struct CumulocityMapperConfig {
    pub in_topic: Topic,
    pub out_topic: Topic,
    pub errors_topic: Topic,
}

impl Default for CumulocityMapperConfig {
    fn default() -> Self {
        Self {
            in_topic: Topic::new("tedge/measurements").unwrap(),
            out_topic: Topic::new("c8y/measurement/measurements/create").unwrap(),
            errors_topic: Topic::new("tedge/errors").unwrap(),
        }
    }
}

impl From<CumulocityMapperConfig> for MapperConfig {
    fn from(value: CumulocityMapperConfig) -> Self {
        MapperConfig {
            in_topic: value.in_topic,
            out_topic: value.out_topic,
            errors_topic: value.errors_topic,
        }
    }
}
