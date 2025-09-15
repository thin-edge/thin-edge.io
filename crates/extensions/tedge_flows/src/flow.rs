use crate::js_runtime::JsRuntime;
use crate::js_script::JsScript;
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
    pub input: FlowInput,

    /// Transformation steps to apply in order to the messages
    pub steps: Vec<FlowStep>,

    pub source: Utf8PathBuf,
}

/// A message transformation step
pub struct FlowStep {
    pub script: JsScript,
    pub config_topics: TopicFilter,
}

pub enum FlowInput {
    MQTT { topics: TopicFilter },
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize, Eq, PartialEq)]
pub struct DateTime {
    pub seconds: u64,
    pub nanoseconds: u32,
}

#[derive(Clone, serde::Deserialize, serde::Serialize, Eq, PartialEq)]
pub struct Message {
    pub topic: String,
    pub payload: Vec<u8>,
    pub timestamp: Option<DateTime>,
}

#[derive(thiserror::Error, Debug)]
pub enum FlowError {
    #[error("Input message cannot be processed: {0}")]
    UnsupportedMessage(String),

    #[error("No messages can be processed due to an incorrect setting: {0}")]
    IncorrectSetting(String),

    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),
}

impl Flow {
    pub fn topics(&self) -> TopicFilter {
        let mut topics = self.input.topics().clone();
        for step in self.steps.iter() {
            topics.add_all(step.config_topics.clone())
        }
        topics
    }

    pub async fn on_config_update(
        &mut self,
        js_runtime: &JsRuntime,
        message: &Message,
    ) -> Result<(), FlowError> {
        for step in self.steps.iter_mut() {
            if step.config_topics.accept_topic_name(&message.topic) {
                step.script.on_config_update(js_runtime, message).await?
            }
        }
        Ok(())
    }

    pub async fn on_message(
        &mut self,
        js_runtime: &JsRuntime,
        stats: &mut Counter,
        timestamp: &DateTime,
        message: &Message,
    ) -> Result<Vec<Message>, FlowError> {
        self.on_config_update(js_runtime, message).await?;
        if !self.input.topics().accept_topic_name(&message.topic) {
            return Ok(vec![]);
        }

        let stated_at = stats.flow_on_message_start(self.source.as_str());
        let mut messages = vec![message.clone()];
        for step in self.steps.iter() {
            let js = step.script.source();
            let mut transformed_messages = vec![];
            for message in messages.iter() {
                let step_started_at = stats.flow_step_start(&js, "onMessage");
                let step_output = step.script.on_message(js_runtime, timestamp, message).await;
                match &step_output {
                    Ok(messages) => {
                        stats.flow_step_done(&js, "onMessage", step_started_at, messages.len())
                    }
                    Err(_) => stats.flow_step_failed(&js, "onMessage"),
                }
                transformed_messages.extend(step_output?);
            }
            messages = transformed_messages;
        }

        stats.flow_on_message_done(self.source.as_str(), stated_at, messages.len());
        Ok(messages)
    }

    pub async fn on_interval(
        &mut self,
        js_runtime: &JsRuntime,
        stats: &mut Counter,
        timestamp: &DateTime,
    ) -> Result<Vec<Message>, FlowError> {
        let stated_at = stats.flow_on_interval_start(self.source.as_str());
        let mut messages = vec![];
        for step in self.steps.iter() {
            let js = step.script.source();
            // Process first the messages triggered upstream by the tick
            let mut transformed_messages = vec![];
            for message in messages.iter() {
                let step_started_at = stats.flow_step_start(&js, "onMessage");
                let step_output = step.script.on_message(js_runtime, timestamp, message).await;
                match &step_output {
                    Ok(messages) => {
                        stats.flow_step_done(&js, "onMessage", step_started_at, messages.len())
                    }
                    Err(_) => stats.flow_step_failed(&js, "onMessage"),
                }
                transformed_messages.extend(step_output?);
            }

            // Only then process the tick
            let step_started_at = stats.flow_step_start(&js, "onInterval");
            let tick_output = step.script.on_interval(js_runtime, timestamp).await;
            match &tick_output {
                Ok(messages) => {
                    stats.flow_step_done(&js, "onInterval", step_started_at, messages.len())
                }
                Err(_) => stats.flow_step_failed(&js, "onInterval"),
            }
            transformed_messages.extend(tick_output?);

            // Iterate with all the messages collected at this step
            messages = transformed_messages;
        }
        stats.flow_on_interval_done(self.source.as_str(), stated_at, messages.len());
        Ok(messages)
    }
}

impl FlowStep {
    pub(crate) fn check(&self, flow: &Utf8Path) {
        let script = &self.script;
        if script.no_js_on_message_fun {
            warn!(target: "flows", "Flow script with no 'onMessage' function: {}", script.path.display());
        }
        if script.no_js_on_config_update_fun && !self.config_topics.is_empty() {
            warn!(target: "flows", "Flow script with no 'onConfigUpdate' function: {}; but configured with 'config_topics' in {flow}", script.path.display());
        }
        if script.no_js_on_interval_fun && script.interval_secs != 0 {
            warn!(target: "flows", "Flow script with no 'onInterval' function: {}; but configured with an 'interval' in {flow}", script.path.display());
        }
    }

    pub(crate) fn fix(&mut self) {
        let script = &mut self.script;
        if !script.no_js_on_interval_fun && script.interval_secs == 0 {
            // 0 as a default is not appropriate for a script with an onInterval handler
            script.interval_secs = 1;
        }
    }
}

impl FlowInput {
    pub fn topics(&self) -> &TopicFilter {
        match self {
            FlowInput::MQTT { topics } => topics,
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
    type Error = FlowError;

    fn try_from(value: OffsetDateTime) -> Result<Self, Self::Error> {
        let seconds = u64::try_from(value.unix_timestamp()).map_err(|err| {
            FlowError::UnsupportedMessage(format!("failed to convert timestamp: {}", err))
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
            payload: payload.to_string().into_bytes(),
            timestamp: Some(DateTime::now()),
        }
    }

    #[cfg(test)]
    pub(crate) fn new_binary(topic: &str, payload: impl Into<Vec<u8>>) -> Self {
        Message {
            topic: topic.to_string(),
            payload: payload.into(),
            timestamp: Some(DateTime::now()),
        }
    }

    pub fn json(&self) -> Value {
        if let Some(timestamp) = &self.timestamp {
            json!({"topic": self.topic, "payload": self.payload, "timestamp": timestamp.json()})
        } else {
            json!({"topic": self.topic, "payload": self.payload, "timestamp": null})
        }
    }

    pub fn payload_str(&self) -> Option<&str> {
        std::str::from_utf8(&self.payload).ok()
    }
}

impl std::fmt::Display for Message {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] ", self.topic)?;
        match &self.payload_str() {
            Some(str) => write!(f, "{str}"),
            None => write!(f, "{:?}", self.payload),
        }
    }
}

impl std::fmt::Debug for Message {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

impl TryFrom<MqttMessage> for Message {
    type Error = FlowError;

    fn try_from(message: MqttMessage) -> Result<Self, Self::Error> {
        let (topic, payload) = message.split();
        Ok(Message {
            topic,
            payload,
            timestamp: None,
        })
    }
}

impl TryFrom<Message> for MqttMessage {
    type Error = FlowError;

    fn try_from(message: Message) -> Result<Self, Self::Error> {
        let topic = message.topic.as_str().try_into().map_err(|_| {
            FlowError::UnsupportedMessage(format!("invalid topic {}", message.topic))
        })?;
        Ok(MqttMessage::new(&topic, message.payload))
    }
}

pub fn error_from_js(err: LoadError) -> FlowError {
    FlowError::IncorrectSetting(format!("{err:#}"))
}
