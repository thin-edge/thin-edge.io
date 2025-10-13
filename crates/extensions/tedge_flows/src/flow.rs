use crate::js_runtime::JsRuntime;
use crate::js_script::JsScript;
use crate::stats::Counter;
use crate::LoadError;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use serde_json::json;
use serde_json::Value;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::Topic;
use tedge_mqtt_ext::TopicFilter;
use tedge_watch_ext::WatchRequest;
use time::OffsetDateTime;
use tokio::time::Instant;
use tracing::error;
use tracing::warn;

/// A chain of message transformations
///
/// A flow consumes messages from a source of type [FlowInput],
/// processes those along a chain of transformation steps,
/// and finally produces the derived messages to a sink of type [FlowOutput].
pub struct Flow {
    /// The message source
    pub input: FlowInput,

    /// Transformation steps to apply in order to the messages
    pub steps: Vec<FlowStep>,

    /// The target for the transformed messages
    pub output: FlowOutput,

    /// The target for error messages
    pub errors: FlowOutput,

    /// Path to the configuration file for this flow
    pub source: Utf8PathBuf,
}

/// A message transformation step
pub struct FlowStep {
    pub script: JsScript,
    pub config_topics: TopicFilter,
}

pub enum FlowInput {
    Mqtt { topics: TopicFilter },
    File { path: Utf8PathBuf },
    Process { command: String },
}

#[derive(Clone)]
pub enum FlowOutput {
    Mqtt { topic: Option<Topic> },
    File { path: Utf8PathBuf },
}

/// The final outcome of a sequence of transformations applied by a flow to a message
pub enum FlowResult {
    Ok {
        flow: String,
        messages: Vec<Message>,
        output: FlowOutput,
    },
    Err {
        flow: String,
        error: FlowError,
        output: FlowOutput,
    },
}

#[derive(Copy, Clone, Debug, serde::Deserialize, serde::Serialize, Eq, PartialEq)]
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
    pub fn name(&self) -> &str {
        self.source.as_str()
    }

    pub fn topics(&self) -> TopicFilter {
        let mut topics = self.input.topics();
        for step in self.steps.iter() {
            topics.add_all(step.config_topics.clone())
        }
        topics
    }

    pub fn watch_request(&self) -> Option<WatchRequest> {
        self.input.watch_request(self.name().to_string())
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
        timestamp: DateTime,
        message: &Message,
    ) -> FlowResult {
        let stated_at = stats.flow_on_message_start(self.name());
        let result = self
            .on_message_steps(js_runtime, stats, timestamp, message)
            .await;
        match &result {
            Ok(messages) => {
                stats.flow_on_message_done(self.name(), stated_at, messages.len());
            }
            Err(_) => stats.flow_on_message_failed(self.name()),
        }
        self.publish(result)
    }

    async fn on_message_steps(
        &mut self,
        js_runtime: &JsRuntime,
        stats: &mut Counter,
        timestamp: DateTime,
        message: &Message,
    ) -> Result<Vec<Message>, FlowError> {
        self.on_config_update(js_runtime, message).await?;
        if !self.input.accept_input_message(self.name(), message) {
            return Ok(vec![]);
        }

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

        Ok(messages)
    }

    pub async fn on_interval(
        &mut self,
        js_runtime: &JsRuntime,
        stats: &mut Counter,
        timestamp: DateTime,
        now: Instant,
    ) -> FlowResult {
        let stated_at = stats.flow_on_interval_start(self.name());
        let result = self
            .on_interval_steps(js_runtime, stats, timestamp, now)
            .await;
        match &result {
            Ok(messages) => {
                stats.flow_on_interval_done(self.name(), stated_at, messages.len());
            }
            Err(_) => stats.flow_on_interval_failed(self.name()),
        }
        self.publish(result)
    }

    async fn on_interval_steps(
        &mut self,
        js_runtime: &JsRuntime,
        stats: &mut Counter,
        timestamp: DateTime,
        now: Instant,
    ) -> Result<Vec<Message>, FlowError> {
        let mut messages = vec![];
        for step in self.steps.iter_mut() {
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

            // Only then process the tick if it's time to execute
            if step.script.should_execute_interval(now) {
                let step_started_at = stats.flow_step_start(&js, "onInterval");
                let tick_output = step.script.on_interval(js_runtime, timestamp).await;
                match &tick_output {
                    Ok(messages) => {
                        stats.flow_step_done(&js, "onInterval", step_started_at, messages.len())
                    }
                    Err(_) => stats.flow_step_failed(&js, "onInterval"),
                }
                transformed_messages.extend(tick_output?);
            }

            // Iterate with all the messages collected at this step
            messages = transformed_messages;
        }
        Ok(messages)
    }

    fn publish(&self, result: Result<Vec<Message>, FlowError>) -> FlowResult {
        match result {
            Ok(messages) => FlowResult::Ok {
                flow: self.name().to_string(),
                messages,
                output: self.output.clone(),
            },
            Err(error) => FlowResult::Err {
                flow: self.name().to_string(),
                error,
                output: self.errors.clone(),
            },
        }
    }
}

impl FlowStep {
    pub(crate) fn check(&self, flow: &Utf8Path) {
        let script = &self.script;
        if script.no_js_on_message_fun {
            warn!(target: "flows", "Flow script with no 'onMessage' function: {}", script.path);
        }
        if script.no_js_on_config_update_fun && !self.config_topics.is_empty() {
            warn!(target: "flows", "Flow script with no 'onConfigUpdate' function: {}; but configured with 'config_topics' in {flow}", script.path);
        }
        if script.no_js_on_interval_fun && !script.interval.is_zero() {
            warn!(target: "flows", "Flow script with no 'onInterval' function: {}; but configured with an 'interval' in {flow}", script.path);
        }
    }

    pub(crate) fn fix(&mut self) {
        let script = &mut self.script;
        if !script.no_js_on_interval_fun && script.interval.is_zero() {
            // Zero as a default is not appropriate for a script with an onInterval handler
            script.interval = std::time::Duration::from_secs(1);
        }
    }
}

impl std::fmt::Display for FlowInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FlowInput::Mqtt { topics } => write!(f, "topics: {topics:?}"),
            FlowInput::File { path } => write!(f, "file: {path}"),
            FlowInput::Process { command } => write!(f, "command: {command}"),
        }
    }
}

impl FlowInput {
    pub fn topics(&self) -> TopicFilter {
        match self {
            FlowInput::Mqtt { topics } => topics.clone(),
            FlowInput::File { .. } | FlowInput::Process { .. } => TopicFilter::empty(),
        }
    }

    pub fn accept_input_message(&self, flow: &str, message: &Message) -> bool {
        match self {
            FlowInput::Mqtt { topics } => topics.accept_topic_name(&message.topic),
            FlowInput::File { .. } | FlowInput::Process { .. } => message.topic == flow,
        }
    }

    pub fn watch_request(&self, flow_name: String) -> Option<WatchRequest> {
        match self {
            FlowInput::Mqtt { .. } => None,
            FlowInput::File { path } => Some(WatchRequest::WatchFile {
                topic: flow_name,
                file: path.to_owned(),
            }),
            FlowInput::Process { command } => Some(WatchRequest::WatchCommand {
                topic: flow_name,
                command: command.to_owned(),
            }),
        }
    }
}

impl DateTime {
    pub fn now() -> Self {
        DateTime::try_from(OffsetDateTime::now_utc()).unwrap()
    }

    pub fn tick_now(&self, tick_every: std::time::Duration) -> bool {
        let tick_every_secs = tick_every.as_secs();
        tick_every_secs != 0 && (self.seconds % tick_every_secs == 0)
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
    pub fn new(topic: impl ToString, payload: impl Into<Vec<u8>>) -> Self {
        Message {
            topic: topic.to_string(),
            payload: payload.into(),
            timestamp: None,
        }
    }

    #[cfg(test)]
    pub fn sent_now(mut self) -> Self {
        self.timestamp = Some(DateTime::now());
        self
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
        write!(
            f,
            "[{}] {}",
            self.topic,
            String::from_utf8_lossy(self.payload.as_ref())
        )
    }
}

impl From<MqttMessage> for Message {
    fn from(message: MqttMessage) -> Self {
        let (topic, payload) = message.split();
        Message::new(topic, payload)
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
