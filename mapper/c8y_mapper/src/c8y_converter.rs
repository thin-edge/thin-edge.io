use mapper_converter::{converter::Converter, error::ConversionError};

pub struct CumulocityConverter;

impl Converter for CumulocityConverter {
    type Error = ConversionError;
    fn convert(&self, input: &[u8]) -> Result<Vec<u8>, Self::Error> {
        c8y_translator_lib::json::from_thin_edge_json(input).map_err(Into::into)
    }
}
