use crate::js_filter::JsFilter;
use crate::js_filter::JsRuntime;
use camino::Utf8PathBuf;
use rustyscript::serde_json::json;
use rustyscript::serde_json::Value;
use rustyscript::Error;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::TopicFilter;
use time::OffsetDateTime;

/// A chain of transformation of MQTT messages
pub struct Pipeline {
    /// The source topics
    pub input_topics: TopicFilter,

    /// Transformation stages to apply in order to the messages
    pub stages: Vec<Stage>,

    pub source: Utf8PathBuf,
}

/// A message transformation stage
pub struct Stage {
    pub filter: JsFilter,
    pub config_topics: TopicFilter,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize, Eq, PartialEq)]
pub struct DateTime {
    seconds: u64,
    nanoseconds: u32,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize, Eq, PartialEq)]
pub struct Message {
    topic: String,
    payload: String,
}

#[derive(thiserror::Error, Debug)]
pub enum FilterError {
    #[error("Input message cannot be processed: {0}")]
    UnsupportedMessage(String),

    #[error("No messages can be processed due to an incorrect setting: {0}")]
    IncorrectSetting(String),
}

impl Pipeline {
    pub fn topics(&self) -> TopicFilter {
        let mut topics = self.input_topics.clone();
        for stage in self.stages.iter() {
            topics.add_all(stage.config_topics.clone())
        }
        topics
    }

    pub fn update_config(
        &mut self,
        js_runtime: &JsRuntime,
        message: &Message,
    ) -> Result<(), FilterError> {
        for stage in self.stages.iter_mut() {
            if stage.config_topics.accept_topic_name(&message.topic) {
                stage.filter.update_config(js_runtime, message)?
            }
        }
        Ok(())
    }

    pub fn process(
        &mut self,
        js_runtime: &JsRuntime,
        timestamp: &DateTime,
        message: &Message,
    ) -> Result<Vec<Message>, FilterError> {
        self.update_config(js_runtime, message)?;
        if !self.input_topics.accept_topic_name(&message.topic) {
            return Ok(vec![]);
        }

        let mut messages = vec![message.clone()];
        for stage in self.stages.iter() {
            let mut transformed_messages = vec![];
            for filter_output in messages
                .iter()
                .map(|message| stage.filter.process(js_runtime, timestamp, message))
            {
                transformed_messages.extend(filter_output?);
            }
            messages = transformed_messages;
        }
        Ok(messages)
    }

    pub fn tick(
        &self,
        js_runtime: &JsRuntime,
        timestamp: &DateTime,
    ) -> Result<Vec<Message>, FilterError> {
        let mut messages = vec![];
        for stage in self.stages.iter() {
            // Process first the messages triggered upstream by the tick
            let mut transformed_messages = vec![];
            for filter_output in messages
                .iter()
                .map(|message| stage.filter.process(js_runtime, timestamp, message))
            {
                transformed_messages.extend(filter_output?);
            }

            // Only then process the tick
            transformed_messages.extend(stage.filter.tick(js_runtime, timestamp)?);

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

    pub fn json(&self) -> Value {
        json!({"seconds": self.seconds, "nanoseconds": self.nanoseconds})
    }

    pub fn tick_now(&self, tick_every_seconds: u64) -> bool {
        tick_every_seconds != 0 && (self.seconds % tick_every_seconds == 0)
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

pub fn error_from_js(err: Error) -> FilterError {
    match err {
        Error::Runtime(err) => FilterError::UnsupportedMessage(err),
        Error::JsError(err) => FilterError::UnsupportedMessage(err.exception_message),
        err => FilterError::IncorrectSetting(format!("{}", err)),
    }
}
