pub trait Converter {
    type Error;

    fn convert(&self, input: &str) -> Result<String, Self::Error>;
}
