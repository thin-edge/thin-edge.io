use mqtt_channel::Message;
use thiserror::Error;

#[derive(Debug)]
pub struct SizeThreshold(pub usize);

impl SizeThreshold {
    pub fn validate(&self, input: &Message) -> Result<(), SizeThresholdExceededError> {
        let size = input.payload_bytes().len();
        let threshold = self.0;
        if size > threshold {
            Err(SizeThresholdExceededError { size, threshold })
        } else {
            Ok(())
        }
    }
}

#[derive(Error, Debug)]
#[error("The payload size of the message is {size}, which is greater than the threshold size of {threshold}.")]
pub struct SizeThresholdExceededError {
    pub size: usize,
    pub threshold: usize,
}
