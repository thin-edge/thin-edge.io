use crate::js_filter::JsFilter;
use crate::js_runtime::JsRuntime;
use crate::LoadError;
use camino::Utf8PathBuf;
use serde_json::json;
use serde_json::Value;
use std::time::Duration;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::TopicFilter;
use time::OffsetDateTime;

/// A chain of transformation of MQTT messages
pub struct Pipeline {
    /// The source topics
    pub input: PipelineInput,

    /// Transformation stages to apply in order to the messages
    pub stages: Vec<Stage>,

    /// Path to pipeline source code
    pub source: Utf8PathBuf,

    /// Target of the transformed messages
    pub output: PipelineOutput,
}

/// A message transformation stage
pub struct Stage {
    pub filter: JsFilter,
    pub config_topics: TopicFilter,
}

pub enum PipelineInput {
    MQTT {
        input_topics: TopicFilter,
    },
    MeaDB {
        input_series: String,
        input_frequency: u64,
        input_span: Duration,
    },
}

pub enum PipelineOutput {
    MQTT { output_topics: TopicFilter },
    MeaDB { output_series: String },
}

#[derive(Clone, Copy, Debug, serde::Deserialize, serde::Serialize, Eq, PartialEq)]
pub struct DateTime {
    pub seconds: u64,
    pub nanoseconds: u32,
}

impl Ord for DateTime {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        use std::cmp::Ordering;
        match self.seconds.cmp(&other.seconds) {
            Ordering::Equal => self.nanoseconds.cmp(&other.nanoseconds),
            ordering => ordering,
        }
    }
}

impl PartialOrd for DateTime {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize, Eq, PartialEq)]
pub struct Message {
    pub topic: String,
    pub payload: String,
}

#[derive(thiserror::Error, Debug)]
pub enum FilterError {
    #[error("Input message cannot be processed: {0}")]
    UnsupportedMessage(String),

    #[error("No messages can be processed due to an incorrect setting: {0}")]
    IncorrectSetting(String),

    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),
}

impl Pipeline {
    pub fn topics(&self) -> TopicFilter {
        match &self.input {
            PipelineInput::MQTT { input_topics } => {
                let mut topics = input_topics.clone();
                for stage in self.stages.iter() {
                    topics.add_all(stage.config_topics.clone())
                }
                topics
            }
            PipelineInput::MeaDB { .. } => TopicFilter::empty(),
        }
    }

    pub fn accept(&self, message_topic: &str) -> bool {
        match &self.input {
            PipelineInput::MQTT { input_topics } => input_topics.accept_topic_name(message_topic),
            PipelineInput::MeaDB { .. } => true,
        }
    }

    pub async fn update_config(
        &mut self,
        js_runtime: &JsRuntime,
        message: &Message,
    ) -> Result<(), FilterError> {
        for stage in self.stages.iter_mut() {
            if stage.config_topics.accept_topic_name(&message.topic) {
                stage.filter.update_config(js_runtime, message).await?
            }
        }
        Ok(())
    }

    pub async fn process(
        &mut self,
        js_runtime: &JsRuntime,
        timestamp: &DateTime,
        message: &Message,
    ) -> Result<Vec<Message>, FilterError> {
        self.update_config(js_runtime, message).await?;
        if !self.accept(&message.topic) {
            return Ok(vec![]);
        }

        let mut messages = vec![message.clone()];
        for stage in self.stages.iter() {
            let mut transformed_messages = vec![];
            for message in messages.iter() {
                let filter_output = stage.filter.process(js_runtime, timestamp, message).await;
                transformed_messages.extend(filter_output?);
            }
            messages = transformed_messages;
        }
        Ok(messages)
    }

    pub async fn tick(
        &mut self,
        js_runtime: &JsRuntime,
        timestamp: &DateTime,
    ) -> Result<Vec<Message>, FilterError> {
        let mut messages = vec![];
        for stage in self.stages.iter() {
            // Process first the messages triggered upstream by the tick
            let mut transformed_messages = vec![];
            for message in messages.iter() {
                let filter_output = stage.filter.process(js_runtime, timestamp, message).await;
                transformed_messages.extend(filter_output?);
            }

            // Only then process the tick
            transformed_messages.extend(stage.filter.tick(js_runtime, timestamp).await?);

            // Iterate with all the messages collected at this stage
            messages = transformed_messages;
        }
        Ok(messages)
    }
}

impl DateTime {
    pub fn now() -> Self {
        DateTime::try_from(OffsetDateTime::now_utc()).unwrap()
    }

    pub fn tick_now(&self, tick_every_seconds: u64) -> bool {
        tick_every_seconds != 0 && (self.seconds % tick_every_seconds == 0)
    }

    pub fn json(&self) -> Value {
        json!({"seconds": self.seconds, "nanoseconds": self.nanoseconds})
    }

    pub fn sub(&self, duration: &Duration) -> Self {
        DateTime {
            seconds: self.seconds - duration.as_secs(),
            nanoseconds: self.nanoseconds,
        }
    }
}

impl TryFrom<OffsetDateTime> for DateTime {
    type Error = FilterError;

    fn try_from(value: OffsetDateTime) -> Result<Self, Self::Error> {
        let seconds = u64::try_from(value.unix_timestamp()).map_err(|err| {
            FilterError::UnsupportedMessage(format!("failed to convert timestamp: {}", err))
        })?;

        Ok(DateTime {
            seconds,
            nanoseconds: value.nanosecond(),
        })
    }
}

impl Message {
    #[cfg(test)]
    pub(crate) fn new(topic: &str, payload: &str) -> Self {
        Message {
            topic: topic.to_string(),
            payload: payload.to_string(),
        }
    }

    pub fn json(&self) -> Value {
        json!({"topic": self.topic, "payload": self.payload})
    }
}

impl TryFrom<MqttMessage> for Message {
    type Error = FilterError;

    fn try_from(message: MqttMessage) -> Result<Self, Self::Error> {
        let topic = message.topic.to_string();
        let payload = message
            .payload_str()
            .map_err(|_| FilterError::UnsupportedMessage("Not an UTF8 payload".to_string()))?
            .to_string();
        Ok(Message { topic, payload })
    }
}

impl TryFrom<Message> for MqttMessage {
    type Error = FilterError;

    fn try_from(message: Message) -> Result<Self, Self::Error> {
        let topic = message.topic.as_str().try_into().map_err(|_| {
            FilterError::UnsupportedMessage(format!("invalid topic {}", message.topic))
        })?;
        Ok(MqttMessage::new(&topic, message.payload))
    }
}

pub fn error_from_js(err: LoadError) -> FilterError {
    FilterError::IncorrectSetting(format!("{err:#}"))
}
