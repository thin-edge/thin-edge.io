pub trait Converter {
    type Error;

    fn convert(&self, input: &str) -> Result<Vec<u8>, Self::Error>;
}
