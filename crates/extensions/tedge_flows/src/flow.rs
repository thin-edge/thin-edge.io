use crate::input_source::PollingSourceError;
use crate::js_runtime::JsRuntime;
use crate::stats::Counter;
use crate::steps::FlowStep;
use crate::LoadError;
use camino::Utf8PathBuf;
use serde_json::json;
use serde_json::Value;
use std::fmt::Display;
use std::fmt::Formatter;
use std::time::Duration;
use std::time::SystemTime;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::QoS;
use tedge_mqtt_ext::Topic;
use tedge_mqtt_ext::TopicFilter;
use tedge_watch_ext::WatchError;
use tokio::time::Instant;

/// A chain of message transformations
///
/// A flow consumes messages from a source of type [FlowInput],
/// processes those along a chain of transformation steps,
/// and finally produces the derived messages to a sink of type [FlowOutput].
pub struct Flow {
    // Name
    pub name: String,

    /// Version
    pub version: Option<String>,

    /// Description of the purpose
    pub description: Option<String>,

    /// User-defined tags
    pub tags: Option<Vec<String>>,

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

pub enum SourceTag {
    /// The message has been received from MQTT
    Mqtt,

    /// The message has been received from a bg process launched by the flow
    Process { flow: String },

    /// The message has been poll by the flow
    Poll { flow: String },
}

#[derive(Clone)]
pub enum FlowInput {
    Mqtt {
        topics: TopicFilter,
    },
    PollFile {
        topic: String,
        path: Utf8PathBuf,
        interval: Duration,
    },
    PollCommand {
        topic: String,
        command: String,
        interval: Duration,
    },
    StreamFile {
        topic: String,
        path: Utf8PathBuf,
    },
    StreamCommand {
        topic: String,
        command: String,
    },
}

#[derive(Clone)]
pub enum FlowOutput {
    Mqtt { topic: Option<Topic> },
    File { path: Utf8PathBuf },
    Context,
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

impl FlowResult {
    pub fn is_err(&self) -> bool {
        match self {
            FlowResult::Ok { .. } => false,
            FlowResult::Err { .. } => true,
        }
    }
}

#[derive(Clone, serde::Deserialize, serde::Serialize, Eq, PartialEq)]
pub struct Message {
    pub topic: String,
    pub payload: Vec<u8>,
    pub timestamp: Option<SystemTime>,
    pub transport: Option<Transport>,
}

#[derive(Clone, serde::Deserialize, serde::Serialize, Eq, PartialEq)]
pub enum Transport {
    Mqtt {
        #[serde(
            serialize_with = "tedge_mqtt_ext::serialize_qos",
            deserialize_with = "tedge_mqtt_ext::deserialize_qos"
        )]
        qos: QoS,
        retain: bool,
    },
}

#[derive(thiserror::Error, Debug)]
pub enum FlowError {
    #[error("Input message cannot be processed: {0}")]
    UnsupportedMessage(String),

    #[error("No messages can be processed due to an incorrect setting: {0}")]
    IncorrectSetting(String),

    #[error(transparent)]
    PollingSourceError(#[from] PollingSourceError),

    #[error(transparent)]
    StreamingSourceError(#[from] WatchError),

    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),
}

impl AsRef<Flow> for Flow {
    fn as_ref(&self) -> &Flow {
        self
    }
}

impl AsMut<Flow> for Flow {
    fn as_mut(&mut self) -> &mut Flow {
        self
    }
}

impl Flow {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn topics(&self) -> TopicFilter {
        self.input.topics()
    }

    pub fn accept_message(&self, _source: &SourceTag, message: &Message) -> bool {
        self.input.accept_message(message)
    }

    pub async fn on_message(
        &mut self,
        js_runtime: &JsRuntime,
        stats: &mut Counter,
        timestamp: SystemTime,
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
        timestamp: SystemTime,
        message: &Message,
    ) -> Result<Vec<Message>, FlowError> {
        let mut messages = vec![message.clone()];
        for step in self.steps.iter_mut() {
            let js = step.source().to_string();
            let mut transformed_messages = vec![];
            for message in messages.iter() {
                let step_started_at = stats.flow_step_start(&js, "onMessage");
                let step_output = step.on_message(js_runtime, timestamp, message).await;
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
        timestamp: SystemTime,
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
        timestamp: SystemTime,
        now: Instant,
    ) -> Result<Vec<Message>, FlowError> {
        let mut messages = vec![];
        for step in self.steps.iter_mut() {
            let js = step.source().to_string();
            // Process first the messages triggered upstream by the tick
            let mut transformed_messages = vec![];
            for message in messages.iter() {
                let step_started_at = stats.flow_step_start(&js, "onMessage");
                let step_output = step.on_message(js_runtime, timestamp, message).await;
                match &step_output {
                    Ok(messages) => {
                        stats.flow_step_done(&js, "onMessage", step_started_at, messages.len())
                    }
                    Err(_) => stats.flow_step_failed(&js, "onMessage"),
                }
                transformed_messages.extend(step_output?);
            }

            // Only then process the tick if it's time to execute
            if step.should_execute_interval(now) {
                let step_started_at = stats.flow_step_start(&js, "onInterval");
                let tick_output = step.on_interval(js_runtime, timestamp).await;
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

    pub fn on_error(&self, error: FlowError) -> FlowResult {
        self.publish(Err(error))
    }

    pub fn publish(&self, result: Result<Vec<Message>, FlowError>) -> FlowResult {
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

impl Display for FlowInput {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            FlowInput::Mqtt { topics } => {
                write!(f, "MQTT topics: {:?}", topics)
            }
            FlowInput::PollFile { path, .. } => {
                write!(f, "Polling file: {path}")
            }
            FlowInput::PollCommand { command, .. } => {
                write!(f, "Polling command: {command}")
            }
            FlowInput::StreamFile { path, .. } => {
                write!(f, "Streaming file: {path}")
            }
            FlowInput::StreamCommand { command, .. } => {
                write!(f, "Streaming command: {command}")
            }
        }
    }
}

impl FlowInput {
    pub fn topics(&self) -> TopicFilter {
        match self {
            FlowInput::Mqtt { topics } => topics.clone(),
            _ => TopicFilter::empty(),
        }
    }

    pub fn enforced_topic(&self) -> Option<&str> {
        match self {
            FlowInput::Mqtt { .. } => None,
            FlowInput::PollFile { topic, .. }
            | FlowInput::PollCommand { topic, .. }
            | FlowInput::StreamFile { topic, .. }
            | FlowInput::StreamCommand { topic, .. } => Some(topic),
        }
    }

    pub fn accept_message(&self, message: &Message) -> bool {
        match self {
            FlowInput::Mqtt { topics } => topics.accept_topic_name(&message.topic),
            FlowInput::PollFile { topic, .. }
            | FlowInput::PollCommand { topic, .. }
            | FlowInput::StreamFile { topic, .. }
            | FlowInput::StreamCommand { topic, .. } => topic == &message.topic,
        }
    }
}

impl Message {
    pub fn new(topic: impl ToString, payload: impl Into<Vec<u8>>) -> Self {
        Message {
            topic: topic.to_string(),
            payload: payload.into(),
            timestamp: None,
            transport: None,
        }
    }

    pub fn with_timestamp(
        topic: impl ToString,
        payload: impl Into<Vec<u8>>,
        timestamp: SystemTime,
    ) -> Self {
        Message {
            topic: topic.to_string(),
            payload: payload.into(),
            timestamp: Some(timestamp),
            transport: None,
        }
    }

    pub fn json(&self) -> Value {
        if let Some(timestamp) = &self.timestamp {
            json!({"topic": self.topic, "payload": self.payload, "timestamp": epoch_ms(timestamp)})
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
        let transport = Transport::Mqtt {
            qos: message.qos,
            retain: message.retain,
        };
        let (topic, payload) = message.split();
        Message {
            topic,
            payload,
            timestamp: None,
            transport: Some(transport),
        }
    }
}

impl TryFrom<Message> for MqttMessage {
    type Error = FlowError;

    fn try_from(message: Message) -> Result<Self, Self::Error> {
        let topic = message.topic.as_str().try_into().map_err(|_| {
            FlowError::UnsupportedMessage(format!("invalid topic {}", message.topic))
        })?;

        let mqtt_message = MqttMessage::new(&topic, message.payload);
        match message.transport {
            Some(Transport::Mqtt { qos, retain }) => {
                Ok(mqtt_message.with_qos(qos).with_retain_flag(retain))
            }
            _ => Ok(mqtt_message),
        }
    }
}

pub(crate) fn epoch_ms(time: &SystemTime) -> u128 {
    let duration = time
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("SystemTime after UNIX EPOCH");
    duration.as_millis()
}

pub fn error_from_js(err: LoadError) -> FlowError {
    FlowError::IncorrectSetting(format!("{err:#}"))
}
