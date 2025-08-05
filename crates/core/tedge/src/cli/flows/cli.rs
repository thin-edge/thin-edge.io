use crate::cli::flows::list::ListCommand;
use crate::cli::flows::test::TestCommand;
use crate::command::BuildCommand;
use crate::command::Command;
use crate::ConfigError;
use anyhow::anyhow;
use anyhow::Context;
use anyhow::Error;
use std::path::PathBuf;
use tedge_config::TEdgeConfig;
use tedge_flows::flow::Message;
use tedge_flows::MessageProcessor;

#[derive(clap::Subcommand, Debug)]
pub enum TEdgeFlowsCli {
    /// List flows and steps
    List {
        /// Path to the directory of flows and steps
        ///
        /// Default to /etc/tedge/flows
        #[clap(long)]
        flows_dir: Option<PathBuf>,

        /// List flows processing messages published on this topic
        ///
        /// If none is provided, lists all the flows
        #[clap(long)]
        topic: Option<String>,
    },

    /// Process message samples
    Test {
        /// Path to the directory of flows and steps
        ///
        /// Default to /etc/tedge/flows
        #[clap(long)]
        flows_dir: Option<PathBuf>,

        /// Path to the flow step script or TOML flow definition
        ///
        /// If none is provided, applies all the matching flows
        #[clap(long)]
        flow: Option<PathBuf>,

        /// Trigger onInterval after all the message samples
        #[clap(long = "final-on-interval")]
        final_on_interval: bool,

        /// Topic of the message sample
        ///
        /// If none is provided, messages are read from stdin expecting a line per message:
        /// [topic] payload
        topic: Option<String>,

        /// Payload of the message sample
        ///
        /// If none is provided, payloads are read from stdin
        payload: Option<String>,
    },
}

impl BuildCommand for TEdgeFlowsCli {
    fn build_command(self, config: &TEdgeConfig) -> Result<Box<dyn Command>, ConfigError> {
        match self {
            TEdgeFlowsCli::List { flows_dir, topic } => {
                let flows_dir = flows_dir.unwrap_or_else(|| Self::default_flows_dir(config));
                Ok(ListCommand { flows_dir, topic }.into_boxed())
            }

            TEdgeFlowsCli::Test {
                flows_dir,
                flow,
                final_on_interval,
                topic,
                payload,
            } => {
                let flows_dir = flows_dir.unwrap_or_else(|| Self::default_flows_dir(config));
                let message = match (topic, payload) {
                    (Some(topic), Some(payload)) => Some(Message {
                        topic,
                        payload,
                        timestamp: None,
                    }),
                    (Some(_), None) => Err(anyhow!("Missing sample payload"))?,
                    (None, Some(_)) => Err(anyhow!("Missing sample topic"))?,
                    (None, None) => None,
                };
                Ok(TestCommand {
                    flows_dir,
                    flow,
                    message,
                    final_on_interval,
                }
                .into_boxed())
            }
        }
    }
}

impl TEdgeFlowsCli {
    fn default_flows_dir(config: &TEdgeConfig) -> PathBuf {
        config.root_dir().join("flows").into()
    }

    pub async fn load_flows(flows_dir: &PathBuf) -> Result<MessageProcessor, Error> {
        MessageProcessor::try_new(flows_dir)
            .await
            .with_context(|| format!("loading flows and steps from {}", flows_dir.display()))
    }

    pub async fn load_file(flows_dir: &PathBuf, path: &PathBuf) -> Result<MessageProcessor, Error> {
        if let Some("toml") = path.extension().and_then(|s| s.to_str()) {
            MessageProcessor::try_new_single_flow(flows_dir, path)
                .await
                .with_context(|| format!("loading flow {flow}", flow = path.display()))
        } else {
            MessageProcessor::try_new_single_step_flow(flows_dir, path)
                .await
                .with_context(|| format!("loading flow script {script}", script = path.display()))
        }
    }
}
