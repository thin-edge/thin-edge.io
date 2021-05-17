use crate::error::*;
use crate::mapper::*;
use async_trait::async_trait;
use chrono::{DateTime, Local};
use mqtt_client::Topic;
use thin_edge_json::json::ThinEdgeJson;
use tokio::task::JoinHandle;
use tracing::{debug, error, instrument};

#[derive(Debug, thiserror::Error)]
pub enum AzureMapperError {
    #[error("The message size is too big. Must be smaller than 255KB.")]
    MessageSizeError,
}

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

impl Into<MapperConfig> for AzureMapperConfig {
    fn into(self) -> MapperConfig {
        MapperConfig {
            in_topic: self.in_topic,
            out_topic: self.out_topic,
            errors_topic: self.errors_topic,
        }
    }
}

pub struct AzureConverter {
    pub(crate) add_timestamp: bool,
}

impl Converter for AzureConverter {
    type Error = ConversionError;
    fn convert(&self, input: &[u8]) -> Result<Vec<u8>, Self::Error> {
        // Size check
        let _ = is_smaller_than_size_threshold(input)?;

        // Validate if a correct Thin Edge JSON or not
        let local_time_now: DateTime<Local> = Local::now();
        let timestamp = local_time_now.with_timezone(local_time_now.offset());
        let valid_thin_edge_json = ThinEdgeJson::from_utf8(input)?;
        println!("{:?}", valid_thin_edge_json);

        // Timestamp config
        if self.add_timestamp && !valid_thin_edge_json.has_timestamp() {
            let local_time_now: DateTime<Local> = Local::now();
            let timestamp = local_time_now.with_timezone(local_time_now.offset());
            // valid_thin_edge_json.set_timestamp(timestamp);
        }

        //Ok(valid_thin_edge_json.serialize()?)
        unimplemented!()
    }
}

fn is_smaller_than_size_threshold(input: &[u8]) -> Result<(), AzureMapperError> {
    let threshold = 255 * 1000; // 255KB
    let size = std::mem::size_of_val(input);
    if size > threshold {
        Err(AzureMapperError::MessageSizeError)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn test_azure_converter() {
        let input_json = r#"{
                  "temperature": 23,
                  "pressure": 220
               }"#;
    }
}
