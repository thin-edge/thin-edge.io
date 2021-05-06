use crate::error::*;
use crate::mapper::*;
use c8y_translator_lib::CumulocityJson;
use mqtt_client::Topic;
use tokio::task::JoinHandle;
use tracing::{debug, error, instrument};

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

impl Into<MapperConfig> for CumulocityMapperConfig {
    fn into(self) -> MapperConfig {
        MapperConfig {
            in_topic: self.in_topic,
            out_topic: self.out_topic,
            errors_topic: self.errors_topic,
        }
    }
}

pub struct CumulocityConverter;

impl Converter for CumulocityConverter {
    type Error = ConversionError;
    fn convert(&self, input: &[u8]) -> Result<Vec<u8>, Self::Error> {
        CumulocityJson::from_thin_edge_json(input).map_err(Into::into)
    }
}
