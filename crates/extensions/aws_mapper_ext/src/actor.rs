use async_trait::async_trait;
use clock::WallClock;
use tedge_actors::Actor;
use tedge_actors::ReceiveMessages;
use tedge_actors::RuntimeError;
use tedge_actors::SimpleMessageBox;
use tedge_mapper_core::size_threshold::SizeThreshold;
use tedge_mqtt_ext::MqttMessage;

use crate::converter::AwsConverter;

#[derive(Debug)]
pub struct AwsMapperActor {
    add_time_stamp: bool,
}

impl AwsMapperActor {
    pub fn new(add_time_stamp: bool) -> Self {
        Self { add_time_stamp }
    }
}

#[async_trait]
impl Actor for AwsMapperActor {
    type MessageBox = SimpleMessageBox<MqttMessage, MqttMessage>;
    fn name(&self) -> &str {
        "AwsMapperActor"
    }

    async fn run(mut self, mut messages: Self::MessageBox) -> Result<(), RuntimeError> {
        let clock = Box::new(WallClock);
        // Quotas at: https://docs.aws.amazon.com/general/latest/gr/iot-core.html#limits_iot
        let size_threshold = SizeThreshold(128 * 1024);
        let mut converter = Box::new(AwsConverter::new(
            self.add_time_stamp,
            clock,
            size_threshold,
        ));

        while let Some(message) = messages.recv().await {
            {
                let converted_messages = converter.convert(&message).await;
                for converted_message in converted_messages.into_iter() {
                    let _ = messages.send(converted_message).await;
                }
            }
        }

        Ok(())
    }
}
