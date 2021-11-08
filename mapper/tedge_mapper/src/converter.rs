pub trait Converter: Send + Sync {
    type Error;

    fn convert(&self, input: &str) -> Result<String, Self::Error>;

    fn convert_child_device_payload(
        &self,
        _input: &str,
        _child_id: &str,
    ) -> Result<String, Self::Error> {
        Ok("".to_string())
    }

    fn convert_child_device_creation(&self, _child_id: &str) -> Option<mqtt_client::Message> {
        None
    }
}
