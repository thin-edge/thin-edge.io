use crate::error::*;
use crate::mapper::*;
use crate::time_provider::TimeProvider;
use mqtt_client::Topic;
use thin_edge_json::serialize::ThinEdgeJsonSerializer;

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
    pub(crate) time_provider: Box<dyn TimeProvider>,
}

impl Converter for AzureConverter {
    type Error = ConversionError;
    fn convert(&self, input: &[u8]) -> Result<Vec<u8>, Self::Error> {
        // Size check
        let threshold = 255 * 1000; // 255KB
        let () = is_smaller_than_size_threshold(input, threshold)?;

        // Add timestamp if necessary
        let default_timestamp = if self.add_timestamp {
            Some(self.time_provider.now())
        } else {
            None
        };

        let mut serializer = ThinEdgeJsonSerializer::new_with_timestamp(default_timestamp);

        let () = thin_edge_json::json::parse_utf8(input, &mut serializer)?;
        Ok(serializer.bytes()?)
    }
}

#[cfg(test)]
mod tests {
    use crate::az_mapper::AzureConverter;
    use crate::mapper::Converter;
    use crate::time_provider::TestTimeProvider;
    use assert_json_diff::*;
    use chrono::{FixedOffset, TimeZone};
    use serde_json::json;

    #[test]
    fn test_azure_converter_invalid_input() {
        let time_provider = TestTimeProvider {
            now: FixedOffset::east(5 * 3600).ymd(2021, 4, 8).and_hms(0, 0, 0),
        };

        let converter = AzureConverter {
            add_timestamp: false,
            time_provider: Box::new(time_provider),
        };

        let input = "This is not Thin Edge JSON";
        let result = converter.convert(input.as_ref());

        assert!(result.is_err());
    }

    #[test]
    fn test_azure_converter_input_no_timestamp_output_no_timestamp() {
        let time_provider = TestTimeProvider {
            now: FixedOffset::east(5 * 3600).ymd(2021, 4, 8).and_hms(0, 0, 0),
        };

        let converter = AzureConverter {
            add_timestamp: false,
            time_provider: Box::new(time_provider),
        };

        let input = r#"{
                  "temperature": 23
               }"#;

        let expected_output = json!({
           "temperature": 23
        });

        let output = converter.convert(input.as_ref());

        assert_json_eq!(
            serde_json::from_slice::<serde_json::Value>(&output.unwrap()).unwrap(),
            expected_output
        );
    }

    #[test]
    fn test_azure_converter_input_has_timestamp_output_has_timestamp_when_add_timestamp_is_false() {
        let time_provider = TestTimeProvider {
            now: FixedOffset::east(5 * 3600).ymd(2021, 4, 8).and_hms(0, 0, 0),
        };

        let converter = AzureConverter {
            add_timestamp: false,
            time_provider: Box::new(time_provider),
        };

        let input = r#"{
                  "time" : "2013-06-22T17:03:14.000+02:00",
                  "temperature": 23
               }"#;

        let expected_output = json!({
           "time" : "2013-06-22T17:03:14+02:00",
           "temperature": 23
        });

        let output = converter.convert(input.as_ref());

        assert_json_eq!(
            serde_json::from_slice::<serde_json::Value>(&output.unwrap()).unwrap(),
            expected_output
        );
    }

    #[test]
    fn test_azure_converter_input_has_timestamp_output_has_timestamp_when_add_timestamp_is_true() {
        let time_provider = TestTimeProvider {
            now: FixedOffset::east(5 * 3600).ymd(2021, 4, 8).and_hms(0, 0, 0),
        };

        let converter = AzureConverter {
            add_timestamp: true,
            time_provider: Box::new(time_provider),
        };

        let input = r#"{
                  "time" : "2013-06-22T17:03:14.000+02:00",
                  "temperature": 23
               }"#;

        let expected_output = json!({
           "time" : "2013-06-22T17:03:14+02:00",
           "temperature": 23
        });

        let output = converter.convert(input.as_ref());

        assert_json_eq!(
            serde_json::from_slice::<serde_json::Value>(&output.unwrap()).unwrap(),
            expected_output
        );
    }

    #[test]
    fn test_azure_converter_input_no_timestamp_output_has_timestamp() {
        let time_provider = TestTimeProvider {
            now: FixedOffset::east(5 * 3600).ymd(2021, 4, 8).and_hms(0, 0, 0),
        };

        let converter = AzureConverter {
            add_timestamp: true,
            time_provider: Box::new(time_provider),
        };

        let input = r#"{
                  "temperature": 23
               }"#;

        let expected_output = json!({
           "temperature": 23,
           "time": "2021-04-08T00:00:00+05:00"
        });

        let output = converter.convert(input.as_ref());

        assert_json_eq!(
            serde_json::from_slice::<serde_json::Value>(&output.unwrap()).unwrap(),
            expected_output
        );
    }
}
