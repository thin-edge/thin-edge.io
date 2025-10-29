use crate::config::ConfigError;
use crate::flow::DateTime;
use crate::flow::Flow;
use crate::flow::FlowError;
use crate::flow::FlowInput;
use crate::flow::FlowResult;
use crate::flow::Message;
use crate::input_source::CommandFlowInput;
use crate::input_source::FileFlowInput;
use crate::input_source::FlowSource;
use crate::input_source::MqttFlowInput;
use crate::registry::FlowRegistry;
use crate::registry::FlowStore;
use camino::Utf8Path;
use tedge_watch_ext::WatchRequest;
use tokio::time::Instant;

/// A flow connected to a source of messages
pub struct ConnectedFlow {
    flow: Flow,
    pub(crate) input: Box<dyn FlowSource>,
}

impl AsRef<Flow> for ConnectedFlow {
    fn as_ref(&self) -> &Flow {
        &self.flow
    }
}

impl AsMut<Flow> for ConnectedFlow {
    fn as_mut(&mut self) -> &mut Flow {
        &mut self.flow
    }
}

impl ConnectedFlow {
    pub fn new(flow: Flow) -> Self {
        let name = flow.name().to_string();
        let input = connect(name, flow.input.clone());
        ConnectedFlow { flow, input }
    }

    pub fn name(&self) -> &str {
        self.flow.name()
    }

    pub fn input_topic(&self) -> &str {
        self.input.enforced_topic().unwrap_or_default()
    }

    pub fn watch_request(&self) -> Option<WatchRequest> {
        self.input.watch_request()
    }

    pub async fn on_source_poll(&mut self, timestamp: DateTime, now: Instant) -> FlowResult {
        let result = self.on_source_poll_steps(timestamp, now).await;
        self.flow.publish(result)
    }

    async fn on_source_poll_steps(
        &mut self,
        timestamp: DateTime,
        now: Instant,
    ) -> Result<Vec<Message>, FlowError> {
        let source = &mut self.input;
        if !source.is_ready(now) {
            return Ok(vec![]);
        };

        let messages = source.poll(timestamp).await?;
        source.update_after_poll(now);
        Ok(messages)
    }

    pub fn on_error(&self, error: FlowError) -> FlowResult {
        self.flow.publish(Err(error))
    }
}

fn connect(flow_name: String, input: FlowInput) -> Box<dyn FlowSource> {
    match input {
        FlowInput::Mqtt { topics } => Box::new(MqttFlowInput { topics }),
        FlowInput::PollFile {
            topic,
            path,
            interval,
        } => Box::new(FileFlowInput::new(flow_name, topic, path, Some(interval))),
        FlowInput::PollCommand {
            topic,
            command,
            interval,
        } => Box::new(CommandFlowInput::new(
            flow_name,
            topic,
            command,
            Some(interval),
        )),
        FlowInput::StreamFile { topic, path } => {
            Box::new(FileFlowInput::new(flow_name, topic, path, None))
        }
        FlowInput::StreamCommand { topic, command } => {
            Box::new(CommandFlowInput::new(flow_name, topic, command, None))
        }
    }
}

pub struct ConnectedFlowRegistry {
    flows: FlowStore<ConnectedFlow>,
}

impl ConnectedFlowRegistry {
    pub fn new(config_dir: impl AsRef<Utf8Path>) -> Self {
        ConnectedFlowRegistry {
            flows: FlowStore::new(config_dir),
        }
    }
}

#[async_trait::async_trait]
impl FlowRegistry for ConnectedFlowRegistry {
    type Flow = ConnectedFlow;

    fn compile(flow: Flow) -> Result<Self::Flow, ConfigError> {
        Ok(ConnectedFlow::new(flow))
    }

    fn store(&self) -> &FlowStore<Self::Flow> {
        &self.flows
    }

    fn store_mut(&mut self) -> &mut FlowStore<Self::Flow> {
        &mut self.flows
    }

    fn deadlines(&self) -> impl Iterator<Item = Instant> + '_ {
        let script_deadlines = self
            .flows
            .flows()
            .flat_map(|flow| &flow.as_ref().steps)
            .filter_map(|step| step.script.next_execution);

        let source_deadlines = self
            .flows
            .flows()
            .filter_map(|flow| flow.input.next_deadline());

        script_deadlines.chain(source_deadlines)
    }
}
