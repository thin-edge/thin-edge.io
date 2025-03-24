use async_trait::async_trait;
use tedge_actors::Actor;
use tedge_actors::MessageReceiver;
use tedge_actors::RuntimeError;
use tedge_actors::SimpleMessageBox;
use tedge_mqtt_ext::MqttMessage;

pub struct WasmMapper {
    messages: SimpleMessageBox<MqttMessage, MqttMessage>,
}

#[async_trait]
impl Actor for WasmMapper {
    fn name(&self) -> &str {
        "WasmMapper"
    }

    async fn run(mut self) -> Result<(), RuntimeError> {
        while let Some(message) = self.messages.recv().await {
            self.filter(message).await;
        }
        Ok(())
    }
}

impl WasmMapper {
    pub fn new(messages: SimpleMessageBox<MqttMessage, MqttMessage>) -> Self {
        Self { messages }
    }

    async fn filter(&mut self, _message: MqttMessage) {}
}
