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
