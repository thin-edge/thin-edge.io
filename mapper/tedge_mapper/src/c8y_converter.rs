use crate::converter::*;
use crate::error::*;
use crate::size_threshold::SizeThreshold;

pub struct CumulocityConverter {
    pub(crate) size_threshold: SizeThreshold,
}

impl Converter for CumulocityConverter {
    type Error = ConversionError;
    fn convert(&self, input: &str) -> Result<String, Self::Error> {
        let () = self.size_threshold.validate(input)?;
        c8y_translator_lib::json::from_thin_edge_json(input).map_err(Into::into)
    }
}

#[cfg(test)]
mod test {
    use super::*;
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
