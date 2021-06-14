use crate::converter::*;
use crate::error::*;

pub struct CumulocityConverter;

impl Converter for CumulocityConverter {
    type Error = ConversionError;
    fn convert(&self, input: &str) -> Result<String, Self::Error> {
        c8y_translator_lib::json::from_thin_edge_json(input).map_err(Into::into)
    }
}
