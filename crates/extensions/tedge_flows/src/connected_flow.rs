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
use crate::params::MapperParams;
use crate::registry::FlowRegistry;
use crate::registry::FlowStore;
use crate::transformers::BuiltinTransformers;
use camino::Utf8Path;
use std::time::SystemTime;
use tedge_watch_ext::WatchRequest;
use tokio::time::Instant;

/// A flow connected to a source of messages
pub struct ConnectedFlow {
    pub(crate) flow: Flow,
    streaming_sources: Vec<Box<dyn StreamingSource>>,
    polling_sources: Vec<Box<dyn PollingSource>>,
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
        let streaming_inputs = flow
            .input
            .iter()
            .filter(|input| input.is_streaming())
            .count();
        let streaming_sources = flow
            .input
            .iter()
            .enumerate()
            .filter_map(|(index, input)| {
                let topic = if streaming_inputs == 1 {
                    flow.source.to_string()
                } else {
                    format!("{}#input-{index}", flow.source)
                };
                streaming_source(topic, input.clone())
            })
            .collect();
        let polling_sources = flow
            .input
            .iter()
            .cloned()
            .filter_map(polling_source)
            .collect();
        ConnectedFlow {
            flow,
            streaming_sources,
            polling_sources,
        }
    }

    pub fn name(&self) -> &str {
        self.flow.name()
    }

    pub fn source_path(&self) -> &Utf8Path {
        &self.flow.source
    }

    pub fn input_topic(&self) -> &str {
        self.flow
            .input
            .iter()
            .find_map(FlowInput::enforced_topic)
            .unwrap_or_default()
    }

    pub fn input_topic_for_watch<'a>(&'a self, topic: &'a str) -> &'a str {
        self.streaming_sources
            .iter()
            .find(|source| {
                source
                    .watch_request()
                    .is_some_and(|request| watch_request_topic(&request) == topic)
            })
            .map_or(topic, |source| source.input_topic())
    }

    pub fn watch_requests(&self) -> Vec<WatchRequest> {
        self.streaming_sources
            .iter()
            .filter_map(|source| source.watch_request())
            .collect()
    }

    pub fn watch_request(&self, topic: &str) -> Option<WatchRequest> {
        self.streaming_sources
            .iter()
            .filter_map(|s| s.watch_request())
            .find(|r| watch_request_topic(r) == topic)
    }

    pub fn next_deadline(&self) -> Option<Instant> {
        self.polling_sources.iter().map(|p| p.next_deadline()).min()
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
        let mut all_messages = Vec::new();
        for source in &mut self.polling_sources {
            if !source.is_ready(now) {
                continue;
            };

            let messages = source.poll(timestamp).await;
            source.update_after_poll(Instant::now());
            all_messages.extend(messages?);
        }
        Ok(all_messages)
    }

    pub fn on_error(&self, error: FlowError) -> FlowResult {
        self.flow.publish(Err(error))
    }
}

pub struct ConnectedFlowRegistry {
    flows: FlowStore<ConnectedFlow>,
    builtins: BuiltinTransformers,
    mapper_params: Box<dyn MapperParams>,
}

impl ConnectedFlowRegistry {
    pub fn new(
        mapper_params: impl MapperParams,
        flows_dir: impl AsRef<Utf8Path>,
    ) -> Result<Self, std::io::Error> {
        Ok(ConnectedFlowRegistry {
            flows: FlowStore::new(flows_dir)?,
            builtins: BuiltinTransformers::default(),
            mapper_params: Box::new(mapper_params),
        })
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

    fn mapper_params(&self) -> &dyn MapperParams {
        self.mapper_params.as_ref()
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

pub(crate) fn watch_request_topic(request: &WatchRequest) -> &str {
    match request {
        WatchRequest::WatchFile { topic, .. }
        | WatchRequest::WatchCommand { topic, .. }
        | WatchRequest::UnWatch { topic } => topic,
    }
}

fn streaming_source(flow_name: String, input: FlowInput) -> Option<Box<dyn StreamingSource>> {
    match input {
        FlowInput::StreamFile { topic, path } => {
            Some(Box::new(FileStreamingSource::new(flow_name, topic, path)))
        }

        FlowInput::StreamCommand {
            topic,
            command,
            cwd,
        } => Some(Box::new(CommandStreamingSource::new(
            flow_name, topic, command, cwd,
        ))),

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
            cwd,
        } => Some(Box::new(CommandPollingSource::new(
            topic, command, cwd, interval,
        ))),

        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flow::FlowOutput;
    use camino::Utf8PathBuf;

    #[test]
    fn multiple_streaming_sources_keep_distinct_message_topics() {
        let flow = Flow {
            name: "test".into(),
            version: None,
            description: None,
            tags: None,
            input: vec![
                FlowInput::StreamCommand {
                    topic: "first".into(),
                    command: "first.sh".into(),
                    cwd: Utf8PathBuf::from("/flows"),
                },
                FlowInput::StreamCommand {
                    topic: "second".into(),
                    command: "second.sh".into(),
                    cwd: Utf8PathBuf::from("/flows"),
                },
            ],
            steps: vec![],
            output: FlowOutput::Mqtt { topic: None },
            errors: FlowOutput::Mqtt { topic: None },
            source: Utf8PathBuf::from("/flows/test.toml"),
            expect_loop: false,
        };

        let connected = ConnectedFlow::new(flow);
        let watch_topics = connected
            .watch_requests()
            .into_iter()
            .map(|request| watch_request_topic(&request).to_string())
            .collect::<Vec<_>>();

        assert_eq!(
            watch_topics,
            vec!["/flows/test.toml#input-0", "/flows/test.toml#input-1"]
        );
        assert_eq!(
            connected.input_topic_for_watch("/flows/test.toml#input-0"),
            "first"
        );
        assert_eq!(
            connected.input_topic_for_watch("/flows/test.toml#input-1"),
            "second"
        );
    }

    #[test]
    fn single_streaming_source_among_mixed_inputs_uses_plain_flow_path_as_watch_topic() {
        let flow = Flow {
            name: "test".into(),
            version: None,
            description: None,
            tags: None,
            input: vec![
                FlowInput::PollCommand {
                    topic: "poll".into(),
                    command: "poll.sh".into(),
                    interval: std::time::Duration::from_secs(5),
                    cwd: Utf8PathBuf::from("/flows"),
                },
                FlowInput::StreamCommand {
                    topic: "stream".into(),
                    command: "stream.sh".into(),
                    cwd: Utf8PathBuf::from("/flows"),
                },
            ],
            steps: vec![],
            output: FlowOutput::Mqtt { topic: None },
            errors: FlowOutput::Mqtt { topic: None },
            source: Utf8PathBuf::from("/flows/test.toml"),
            expect_loop: false,
        };

        let connected = ConnectedFlow::new(flow);
        let watch_topics = connected
            .watch_requests()
            .into_iter()
            .map(|request| watch_request_topic(&request).to_string())
            .collect::<Vec<_>>();

        // One streaming input → plain flow path, no #input-N suffix
        assert_eq!(watch_topics, vec!["/flows/test.toml"]);
        assert_eq!(
            connected.input_topic_for_watch("/flows/test.toml"),
            "stream"
        );
    }

    #[test]
    fn input_topic_for_watch_returns_watch_topic_itself_when_no_source_matches() {
        let flow = Flow {
            name: "test".into(),
            version: None,
            description: None,
            tags: None,
            input: vec![FlowInput::StreamCommand {
                topic: "stream".into(),
                command: "stream.sh".into(),
                cwd: Utf8PathBuf::from("/flows"),
            }],
            steps: vec![],
            output: FlowOutput::Mqtt { topic: None },
            errors: FlowOutput::Mqtt { topic: None },
            source: Utf8PathBuf::from("/flows/test.toml"),
            expect_loop: false,
        };

        let connected = ConnectedFlow::new(flow);
        assert_eq!(
            connected.input_topic_for_watch("unknown/watch/topic"),
            "unknown/watch/topic"
        );
    }
}
