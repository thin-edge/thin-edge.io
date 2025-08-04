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
use tedge_gen_mapper::flow::Message;
use tedge_gen_mapper::MessageProcessor;

#[derive(clap::Subcommand, Debug)]
pub enum TEdgeFlowsCli {
    /// List flows and steps
    List {
        /// Path to the directory of flows and steps
        ///
        /// Default to /etc/tedge/flows
        #[clap(long)]
        mapping_dir: Option<PathBuf>,

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
        mapping_dir: Option<PathBuf>,

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
            TEdgeFlowsCli::List { mapping_dir, topic } => {
                let mapping_dir = mapping_dir.unwrap_or_else(|| Self::default_mapping_dir(config));
                Ok(ListCommand { mapping_dir, topic }.into_boxed())
            }

            TEdgeFlowsCli::Test {
                mapping_dir,
                flow,
                final_on_interval,
                topic,
                payload,
            } => {
                let mapping_dir = mapping_dir.unwrap_or_else(|| Self::default_mapping_dir(config));
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
                    mapping_dir,
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
    fn default_mapping_dir(config: &TEdgeConfig) -> PathBuf {
        config.root_dir().join("flows").into()
    }

    pub async fn load_flows(mapping_dir: &PathBuf) -> Result<MessageProcessor, Error> {
        MessageProcessor::try_new(mapping_dir)
            .await
            .with_context(|| format!("loading flows and steps from {}", mapping_dir.display()))
    }

    pub async fn load_file(
        mapping_dir: &PathBuf,
        path: &PathBuf,
    ) -> Result<MessageProcessor, Error> {
        if let Some("toml") = path.extension().and_then(|s| s.to_str()) {
            MessageProcessor::try_new_single_flow(mapping_dir, path)
                .await
                .with_context(|| format!("loading flow {flow}", flow = path.display()))
        } else {
            MessageProcessor::try_new_single_step_flow(mapping_dir, path)
                .await
                .with_context(|| format!("loading flow script {script}", script = path.display()))
        }
    }
}
