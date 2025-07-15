use crate::js_filter::JsFilter;
use crate::js_runtime::JsRuntime;
use crate::stats::Counter;
use crate::LoadError;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use serde_json::json;
use serde_json::Value;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::TopicFilter;
use time::OffsetDateTime;
use tracing::warn;

/// A chain of transformation of MQTT messages
pub struct Flow {
    /// The source topics
    pub input_topics: TopicFilter,

    /// Transformation steps to apply in order to the messages
    pub steps: Vec<FlowStep>,

    pub source: Utf8PathBuf,
}

/// A message transformation step
pub struct FlowStep {
    pub filter: JsFilter,
    pub config_topics: TopicFilter,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize, Eq, PartialEq)]
pub struct DateTime {
    pub seconds: u64,
    pub nanoseconds: u32,
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

impl Flow {
    pub fn topics(&self) -> TopicFilter {
        let mut topics = self.input_topics.clone();
        for step in self.steps.iter() {
            topics.add_all(step.config_topics.clone())
        }
        topics
    }

    pub async fn update_config(
        &mut self,
        js_runtime: &JsRuntime,
        message: &Message,
    ) -> Result<(), FilterError> {
        for step in self.steps.iter_mut() {
            if step.config_topics.accept_topic_name(&message.topic) {
                step.filter.update_config(js_runtime, message).await?
            }
        }
        Ok(())
    }

    pub async fn process(
        &mut self,
        js_runtime: &JsRuntime,
        stats: &mut Counter,
        timestamp: &DateTime,
        message: &Message,
    ) -> Result<Vec<Message>, FilterError> {
        self.update_config(js_runtime, message).await?;
        if !self.input_topics.accept_topic_name(&message.topic) {
            return Ok(vec![]);
        }

        let stated_at = stats.flow_process_start(self.source.as_str());
        let mut messages = vec![message.clone()];
        for step in self.steps.iter() {
            let js = step.filter.source();
            let mut transformed_messages = vec![];
            for message in messages.iter() {
                let filter_started_at = stats.filter_start(&js, "process");
                let filter_output = step.filter.process(js_runtime, timestamp, message).await;
                match &filter_output {
                    Ok(messages) => {
                        stats.filter_done(&js, "process", filter_started_at, messages.len())
                    }
                    Err(_) => stats.filter_failed(&js, "process"),
                }
                transformed_messages.extend(filter_output?);
            }
            messages = transformed_messages;
        }

        stats.flow_process_done(self.source.as_str(), stated_at, messages.len());
        Ok(messages)
    }

    pub async fn tick(
        &mut self,
        js_runtime: &JsRuntime,
        stats: &mut Counter,
        timestamp: &DateTime,
    ) -> Result<Vec<Message>, FilterError> {
        let stated_at = stats.flow_tick_start(self.source.as_str());
        let mut messages = vec![];
        for step in self.steps.iter() {
            let js = step.filter.source();
            // Process first the messages triggered upstream by the tick
            let mut transformed_messages = vec![];
            for message in messages.iter() {
                let filter_started_at = stats.filter_start(&js, "process");
                let filter_output = step.filter.process(js_runtime, timestamp, message).await;
                match &filter_output {
                    Ok(messages) => {
                        stats.filter_done(&js, "process", filter_started_at, messages.len())
                    }
                    Err(_) => stats.filter_failed(&js, "process"),
                }
                transformed_messages.extend(filter_output?);
            }

            // Only then process the tick
            let filter_started_at = stats.filter_start(&js, "tick");
            let tick_output = step.filter.tick(js_runtime, timestamp).await;
            match &tick_output {
                Ok(messages) => stats.filter_done(&js, "tick", filter_started_at, messages.len()),
                Err(_) => stats.filter_failed(&js, "tick"),
            }
            transformed_messages.extend(tick_output?);

            // Iterate with all the messages collected at this step
            messages = transformed_messages;
        }
        stats.flow_tick_done(self.source.as_str(), stated_at, messages.len());
        Ok(messages)
    }
}

impl FlowStep {
    pub(crate) fn check(&self, flow: &Utf8Path) {
        let filter = &self.filter;
        if filter.no_js_process {
            warn!(target: "MAPPING", "Filter with no 'process' function: {}", filter.path.display());
        }
        if filter.no_js_update_config && !self.config_topics.is_empty() {
            warn!(target: "MAPPING", "Filter with no 'config_update' function: {}; but configured with 'config_topics' in {flow}", filter.path.display());
        }
        if filter.no_js_tick && filter.tick_every_seconds != 0 {
            warn!(target: "MAPPING", "Filter with no 'tick' function: {}; but configured with 'tick_every_seconds' in {flow}", filter.path.display());
        }
    }

    pub(crate) fn fix(&mut self) {
        let filter = &mut self.filter;
        if !filter.no_js_tick && filter.tick_every_seconds == 0 {
            // 0 as a default is not appropriate for a filter with a tick handler
            filter.tick_every_seconds = 1;
        }
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
