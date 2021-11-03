pub trait Converter: Send + Sync {
    type Error;

    fn convert(&self, input: &str) -> Result<String, Self::Error>;
    fn convert_to_child_device(&self, input: &str, child_id: &str) -> Result<String, Self::Error> {
        Ok("".to_string())
    }
}
