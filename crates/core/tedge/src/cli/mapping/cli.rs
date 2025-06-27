use crate::cli::mapping::list::ListCommand;
use crate::cli::mapping::test::TestCommand;
use crate::command::BuildCommand;
use crate::command::Command;
use crate::ConfigError;
use anyhow::anyhow;
use anyhow::Context;
use anyhow::Error;
use std::path::PathBuf;
use tedge_config::TEdgeConfig;
use tedge_gen_mapper::pipeline::Message;
use tedge_gen_mapper::MessageProcessor;

#[derive(clap::Subcommand, Debug)]
pub enum TEdgeMappingCli {
    /// List pipelines and filters
    List {
        /// Path to pipeline and filter specs
        ///
        /// Default to /etc/tedge/gen-mapper
        #[clap(long)]
        mapping_dir: Option<PathBuf>,

        /// List pipelines processing messages published on this topic
        ///
        /// If none is provided, lists all the pipelines
        #[clap(long)]
        topic: Option<String>,
    },

    /// Process message samples
    Test {
        /// Path to pipeline and filter specs
        ///
        /// Default to /etc/tedge/gen-mapper
        #[clap(long)]
        mapping_dir: Option<PathBuf>,

        /// Path to the javascript filter or TOML pipeline definition
        ///
        /// If none is provided, applies all the matching pipelines
        #[clap(long)]
        filter: Option<PathBuf>,

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

impl BuildCommand for TEdgeMappingCli {
    fn build_command(self, config: &TEdgeConfig) -> Result<Box<dyn Command>, ConfigError> {
        match self {
            TEdgeMappingCli::List { mapping_dir, topic } => {
                let mapping_dir = mapping_dir.unwrap_or_else(|| Self::default_mapping_dir(config));
                Ok(ListCommand { mapping_dir, topic }.into_boxed())
            }

            TEdgeMappingCli::Test {
                mapping_dir,
                filter,
                topic,
                payload,
            } => {
                let mapping_dir = mapping_dir.unwrap_or_else(|| Self::default_mapping_dir(config));
                let message = match (topic, payload) {
                    (Some(topic), Some(payload)) => Some(Message { topic, payload }),
                    (Some(_), None) => Err(anyhow!("Missing sample payload"))?,
                    (None, Some(_)) => Err(anyhow!("Missing sample topic"))?,
                    (None, None) => None,
                };
                Ok(TestCommand {
                    mapping_dir,
                    filter,
                    message,
                }
                .into_boxed())
            }
        }
    }
}

impl TEdgeMappingCli {
    fn default_mapping_dir(config: &TEdgeConfig) -> PathBuf {
        config.root_dir().join("gen-mapper").into()
    }

    pub async fn load_pipelines(mapping_dir: &PathBuf) -> Result<MessageProcessor, Error> {
        MessageProcessor::try_new(mapping_dir)
            .await
            .with_context(|| {
                format!(
                    "loading pipelines and filters from {}",
                    mapping_dir.display()
                )
            })
    }
}
