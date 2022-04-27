use c8y_smartrest::error::SmartRestSerializerError;
use c8y_smartrest::smartrest_serializer::SmartRest;
use c8y_smartrest::topic::C8yTopic;
use mqtt_channel::Message;

pub trait TryIntoOperationStatusMessage {
    fn executing() -> Result<Message, SmartRestSerializerError> {
        let status = Self::status_executing()?;
        Ok(Self::create_message(status))
    }

    fn successful(parameter: Option<String>) -> Result<Message, SmartRestSerializerError> {
        let status = Self::status_successful(parameter)?;
        Ok(Self::create_message(status))
    }

    fn failed(failure_reason: String) -> Result<Message, SmartRestSerializerError> {
        let status = Self::status_failed(failure_reason)?;
        Ok(Self::create_message(status))
    }

    fn create_message(payload: SmartRest) -> Message {
        let topic = C8yTopic::SmartRestResponse.to_topic().unwrap(); // never fail
        Message::new(&topic, payload)
    }

    fn status_executing() -> Result<SmartRest, SmartRestSerializerError>;
    fn status_successful(parameter: Option<String>) -> Result<SmartRest, SmartRestSerializerError>;
    fn status_failed(failure_reason: String) -> Result<SmartRest, SmartRestSerializerError>;
}
