use crate::config::ConfigError;
use crate::flow::Flow;
use crate::flow::FlowError;
use crate::flow::FlowInput;
use crate::flow::FlowResult;
use crate::flow::Message;
use crate::input_source::CommandPollingSource;
use crate::input_source::CommandStreamingSource;
use crate::input_source::FilePollingSource;
use crate::input_source::FileStreamingSource;
use crate::input_source::PollingSource;
use crate::input_source::StreamingSource;
use crate::registry::FlowRegistry;
use crate::registry::FlowStore;
use crate::transformers::BuiltinTransformers;
use camino::Utf8Path;
use std::time::SystemTime;
use tedge_watch_ext::WatchRequest;
use tokio::time::Instant;

/// A flow connected to a source of messages
pub struct ConnectedFlow {
    flow: Flow,
    streaming_source: Option<Box<dyn StreamingSource>>,
    polling_source: Option<Box<dyn PollingSource>>,
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
        let streaming_source = streaming_source(flow.name().to_owned(), flow.input.clone());
        let polling_source = polling_source(flow.input.clone());
        ConnectedFlow {
            flow,
            streaming_source,
            polling_source,
        }
    }

    pub fn name(&self) -> &str {
        self.flow.name()
    }

    pub fn input_topic(&self) -> &str {
        self.flow.input.enforced_topic().unwrap_or_default()
    }

    pub fn watch_request(&self) -> Option<WatchRequest> {
        self.streaming_source
            .as_ref()
            .and_then(|source| source.watch_request())
    }

    pub fn next_deadline(&self) -> Option<Instant> {
        self.polling_source.as_ref().and_then(|p| p.next_deadline())
    }

    pub async fn on_source_poll(&mut self, timestamp: SystemTime, now: Instant) -> FlowResult {
        let result = self.on_source_poll_steps(timestamp, now).await;
        self.flow.publish(result)
    }

    async fn on_source_poll_steps(
        &mut self,
        timestamp: SystemTime,
        now: Instant,
    ) -> Result<Vec<Message>, FlowError> {
        let Some(source) = &mut self.polling_source.as_mut() else {
            return Ok(vec![]);
        };
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

fn streaming_source(flow_name: String, input: FlowInput) -> Option<Box<dyn StreamingSource>> {
    match input {
        FlowInput::StreamFile { topic: _, path } => {
            Some(Box::new(FileStreamingSource::new(flow_name, path)))
        }

        FlowInput::StreamCommand { topic: _, command } => {
            Some(Box::new(CommandStreamingSource::new(flow_name, command)))
        }

        _ => None,
    }
}

fn polling_source(input: FlowInput) -> Option<Box<dyn PollingSource>> {
    match input {
        FlowInput::PollFile {
            topic,
            path,
            interval,
        } => Some(Box::new(FilePollingSource::new(topic, path, interval))),

        FlowInput::PollCommand {
            topic,
            command,
            interval,
        } => Some(Box::new(CommandPollingSource::new(
            topic, command, interval,
        ))),

        _ => None,
    }
}

pub struct ConnectedFlowRegistry {
    flows: FlowStore<ConnectedFlow>,
    builtins: BuiltinTransformers,
}

impl ConnectedFlowRegistry {
    pub fn new(config_dir: impl AsRef<Utf8Path>) -> Self {
        ConnectedFlowRegistry {
            flows: FlowStore::new(config_dir),
            builtins: BuiltinTransformers::default(),
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

    fn builtins(&self) -> &BuiltinTransformers {
        &self.builtins
    }

    fn builtins_mut(&mut self) -> &mut BuiltinTransformers {
        &mut self.builtins
    }

    fn deadlines(&self) -> impl Iterator<Item = Instant> + '_ {
        let script_deadlines = self
            .flows
            .flows()
            .flat_map(|flow| &flow.as_ref().steps)
            .filter_map(|step| step.next_execution);

        let source_deadlines = self.flows.flows().filter_map(|flow| flow.next_deadline());

        script_deadlines.chain(source_deadlines)
    }
}
