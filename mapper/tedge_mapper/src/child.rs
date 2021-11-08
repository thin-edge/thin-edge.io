use crate::error::ConversionError;
use async_trait::async_trait;
use mqtt_client::MqttClientError;

#[async_trait]
pub trait ChildSupport: Send + Sync {}
