use std::sync::Arc;

use async_trait::async_trait;
use mockall::automock;
use mqtt_client::{Client, ErrorStream, Message, MessageId, MessageStream, TopicFilter};

#[automock]
#[async_trait]
pub trait MqttClient: Send + Sync {
    fn subscribe_errors(&self) -> Box<dyn MqttErrorStream>;

    async fn subscribe(
        &self,
        filter: TopicFilter,
    ) -> Result<Box<dyn MqttMessageStream>, mqtt_client::Error>;

    async fn publish(&self, message: Message) -> Result<MessageId, mqtt_client::Error>;
}

#[async_trait]
#[automock]
pub trait MqttMessageStream: Send + Sync {
    async fn next(&mut self) -> Option<Message>;
}

pub struct MqttMessageStreamImpl {
    message_stream: MessageStream,
}

#[async_trait]
impl MqttMessageStream for MqttMessageStreamImpl {
    async fn next(&mut self) -> Option<Message> {
        self.message_stream.next().await
    }
}

#[automock]
#[async_trait]
pub trait MqttErrorStream: Send + Sync {
    async fn next(&mut self) -> Option<Arc<mqtt_client::Error>>;
}

pub struct MqttErrorStreamImpl {
    error_stream: ErrorStream,
}

#[async_trait]
impl MqttErrorStream for MqttErrorStreamImpl {
    async fn next(&mut self) -> Option<Arc<mqtt_client::Error>> {
        self.error_stream.next().await
    }
}

pub struct MqttClientImpl {
    pub mqtt_client: Client,
}

#[async_trait]
impl MqttClient for MqttClientImpl {
    fn subscribe_errors(&self) -> Box<dyn MqttErrorStream> {
        let error_stream = self.mqtt_client.subscribe_errors();
        Box::new(MqttErrorStreamImpl { error_stream })
    }

    async fn subscribe(
        &self,
        filter: TopicFilter,
    ) -> Result<Box<dyn MqttMessageStream>, mqtt_client::Error> {
        let message_stream = self.mqtt_client.subscribe(filter).await?;
        Ok(Box::new(MqttMessageStreamImpl { message_stream }))
    }

    async fn publish(&self, message: Message) -> Result<MessageId, mqtt_client::Error> {
        self.mqtt_client.publish(message).await
    }
}
