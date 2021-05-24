use crate::size_threshold::SizeThreshold;
use clock::Clock;
use mapper_converter::{converter::Converter, error::ConversionError};
use thin_edge_json::serialize::ThinEdgeJsonSerializer;

pub struct AzureConverter {
    pub add_timestamp: bool,
    pub clock: Box<dyn Clock>,
    pub size_threshold: SizeThreshold,
}

impl Converter for AzureConverter {
    type Error = ConversionError;
    fn convert(&self, input: &[u8]) -> Result<Vec<u8>, Self::Error> {
        // let () = self.size_threshold.validate(input)?;
        let () = self.size_threshold.validate(input).unwrap();

        let default_timestamp = self.add_timestamp.then(|| self.clock.now());

        let mut serializer = ThinEdgeJsonSerializer::new_with_timestamp(default_timestamp);

        let () = thin_edge_json::json::parse_utf8(input, &mut serializer)?;
        Ok(serializer.bytes()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::size_threshold::SizeThresholdExceeded;
    use assert_json_diff::*;
    use assert_matches::*;
    use chrono::{FixedOffset, TimeZone};
    use serde_json::json;

    struct TestClock;

    impl Clock for TestClock {
        fn now(&self) -> clock::Timestamp {
            FixedOffset::east(5 * 3600).ymd(2021, 4, 8).and_hms(0, 0, 0)
        }
    }

    #[test]
    fn converting_invalid_json_is_invalid() {
        let converter = AzureConverter {
            add_timestamp: false,
            clock: Box::new(TestClock),
            size_threshold: SizeThreshold(255 * 1024),
        };

        let input = "This is not Thin Edge JSON";
        let result = converter.convert(input.as_ref());

        assert!(result.is_err());
    }

    #[test]
    fn converting_input_without_timestamp_produces_output_without_timestamp_given_add_timestamp_is_false(
    ) {
        let converter = AzureConverter {
            add_timestamp: false,
            clock: Box::new(TestClock),
            size_threshold: SizeThreshold(255 * 1024),
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
    fn converting_input_with_timestamp_produces_output_with_timestamp_given_add_timestamp_is_false()
    {
        let converter = AzureConverter {
            add_timestamp: false,
            clock: Box::new(TestClock),
            size_threshold: SizeThreshold(255 * 1024),
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
    fn converting_input_with_timestamp_produces_output_with_timestamp_given_add_timestamp_is_true()
    {
        let converter = AzureConverter {
            add_timestamp: true,
            clock: Box::new(TestClock),
            size_threshold: SizeThreshold(255 * 1024),
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
    fn converting_input_without_timestamp_produces_output_with_timestamp_given_add_timestamp_is_true(
    ) {
        let converter = AzureConverter {
            add_timestamp: true,
            clock: Box::new(TestClock),
            size_threshold: SizeThreshold(255 * 1024),
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

    // #[test]
    // fn exceeding_threshold_returns_error() {
    //     let converter = AzureConverter {
    //         add_timestamp: false,
    //         clock: Box::new(TestClock),
    //         size_threshold: SizeThreshold(1),
    //     };

    //     let input = "ABC";
    //     let result = converter.convert(input.as_ref());

    //     assert_matches!(
    //         result,
    //         Err(ConversionError::MessageSizeExceededError(
    //             SizeThresholdExceeded {
    //                 actual_size: 3,
    //                 threshold: 1
    //             }
    //         ))
    //     );
    // }
}
