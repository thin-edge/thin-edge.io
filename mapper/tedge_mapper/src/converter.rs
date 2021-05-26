pub trait Converter {
    type Error;

    fn convert(&self, input: &[u8]) -> Result<Vec<u8>, Self::Error>;
}
