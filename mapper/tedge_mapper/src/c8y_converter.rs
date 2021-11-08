use crate::converter::*;
use crate::error::*;
use crate::size_threshold::SizeThreshold;
use mqtt_client::{Message, Topic};

pub struct CumulocityConverter {
    pub(crate) size_threshold: SizeThreshold,
}

impl Converter for CumulocityConverter {
    type Error = ConversionError;
    fn convert(&self, input: &str) -> Result<String, Self::Error> {
        let () = self.size_threshold.validate(input)?;
        c8y_translator_lib::json::from_thin_edge_json(input).map_err(Into::into)
    }

    fn convert_child_device_payload(
        &self,
        input: &str,
        child_id: &str,
    ) -> Result<String, Self::Error> {
        let () = self.size_threshold.validate(input)?;
        c8y_translator_lib::json::from_thin_edge_json_with_child(input, child_id)
            .map_err(Into::into)
    }

    fn convert_child_device_creation(&self, child_id: &str) -> Option<Message> {
        Some(Message::new(
            &Topic::new("c8y/s/us").unwrap(),
            format!("101,{},{},thin-edge.io-child", child_id, child_id),
        ))
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use test_case::test_case;

    #[test_case("child1", "c8y/s/us", "101,child1,child1,thin-edge.io-child"; "smartrest")]
    fn child_device_creation(child_id: &str, expected_topic: &str, expected_payload: &str) {
        let expected_message = Message::new(&Topic::new(expected_topic).unwrap(), expected_payload);
        let converter = Box::new(CumulocityConverter {
            size_threshold: SizeThreshold(16 * 1024),
        });
        let message = converter.convert_child_device_creation(child_id).unwrap();
        assert_eq!(message, expected_message)
    }

    #[test]
    fn check_c8y_threshold_packet_size() -> Result<(), anyhow::Error> {
        let size_threshold = SizeThreshold(16 * 1024);
        let converter = CumulocityConverter { size_threshold };
        let buffer = create_packet(1024 * 20);
        let err = converter.size_threshold.validate(&buffer).unwrap_err();
        assert_eq!(
            err.to_string(),
            "The input size 20480 is too big. The threshold is 16384."
        );
        Ok(())
    }

    fn create_packet(size: usize) -> String {
        let data: String = "Some data!".into();
        let loops = size / data.len();
        let mut buffer = String::with_capacity(size);
        for _ in 0..loops {
            buffer.push_str("Some data!");
        }
        buffer
    }
}
