pub trait Converter: Send + Sync {
    type Error;

    fn convert(&self, input: &str) -> Result<String, Self::Error>;
}
