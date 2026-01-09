use crate::cli::flows::list::ListCommand;
use crate::cli::flows::test::TestCommand;
use crate::command::BuildCommand;
use crate::command::Command;
use crate::log::MaybeFancy;
use crate::ConfigError;
use anyhow::anyhow;
use anyhow::Context;
use anyhow::Error;
use camino::Utf8PathBuf;
use std::time::SystemTime;
use tedge_config::TEdgeConfig;
use tedge_flows::BaseFlowRegistry;
use tedge_flows::Message;
use tedge_flows::MessageProcessor;

#[derive(clap::Subcommand, Debug)]
pub enum TEdgeFlowsCli {
    /// List flows and steps
    List {
        /// Mapper name
        #[clap(long, default_value = "flows", global = true)]
        mapper: String,

        /// Mapper profile
        #[clap(long, global = true)]
        profile: Option<String>,

        /// Path to the directory of flows and steps
        ///
        /// Default to /etc/tedge/mappers/$MAPPER.$PROFILE/flows
        #[clap(long, global = true)]
        flows_dir: Option<Utf8PathBuf>,

        /// List flows processing messages published on this topic
        ///
        /// If none is provided, lists all the flows
        #[clap(long)]
        topic: Option<String>,
    },

    /// Process message samples
    Test {
        /// Mapper name
        #[clap(long, default_value = "flows", global = true)]
        mapper: String,

        /// Mapper profile
        #[clap(long, global = true)]
        profile: Option<String>,

        /// Path to the directory of flows and steps
        ///
        /// Default to /etc/tedge/mappers/$MAPPER.$PROFILE/flows
        #[clap(long, global = true)]
        flows_dir: Option<Utf8PathBuf>,

        /// Path to the flow step script or TOML flow definition
        ///
        /// If none is provided, applies all the matching flows
        #[clap(long)]
        flow: Option<Utf8PathBuf>,

        /// Trigger onInterval after all the message samples
        #[clap(long = "final-on-interval")]
        final_on_interval: bool,

        /// Processing time to be used for the test
        #[clap(long = "processing-time")]
        #[arg(value_parser = humantime::parse_rfc3339_weak)]
        processing_time: Option<SystemTime>,

        /// The input payloads are base64 encoded and have to be decoded first
        #[clap(long = "base64-input")]
        base64_input: bool,

        /// The output payloads have to be base64 encoded before being displayed
        #[clap(long = "base64-output")]
        base64_output: bool,

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

    /// Display the path to the directory of flows and steps
    ConfigDir {
        /// Mapper name
        #[clap(long, default_value = "flows", global = true)]
        mapper: String,

        /// Mapper profile
        #[clap(long, global = true)]
        profile: Option<String>,
    },
}

#[async_trait::async_trait]
impl BuildCommand for TEdgeFlowsCli {
    async fn build_command(self, config: &TEdgeConfig) -> Result<Box<dyn Command>, ConfigError> {
        match self {
            TEdgeFlowsCli::List {
                mapper,
                profile,
                flows_dir,
                topic,
            } => {
                let flows_dir = flows_dir.unwrap_or_else(|| {
                    Self::default_flows_dir(config, &mapper, profile.as_deref())
                });
                Ok(ListCommand { flows_dir, topic }.into_boxed())
            }

            TEdgeFlowsCli::Test {
                mapper,
                profile,
                flows_dir,
                flow,
                final_on_interval,
                processing_time,
                base64_input,
                base64_output,
                topic,
                payload,
            } => {
                let flows_dir = flows_dir.unwrap_or_else(|| {
                    Self::default_flows_dir(config, &mapper, profile.as_deref())
                });
                let message = match (topic, payload) {
                    (Some(topic), Some(payload)) => Some(Message::new(topic, payload)),
                    (Some(_), None) => Err(anyhow!("Missing sample payload"))?,
                    (None, Some(_)) => Err(anyhow!("Missing sample topic"))?,
                    (None, None) => None,
                };
                Ok(TestCommand {
                    flows_dir,
                    flow,
                    message,
                    final_on_interval,
                    processing_time,
                    base64_input,
                    base64_output,
                }
                .into_boxed())
            }

            TEdgeFlowsCli::ConfigDir { mapper, profile } => {
                let flows_dir = Self::default_flows_dir(config, &mapper, profile.as_deref());
                Ok(ConfigDirCommand { flows_dir }.into_boxed())
            }
        }
    }
}

impl TEdgeFlowsCli {
    fn default_flows_dir(config: &TEdgeConfig, mapper: &str, profile: Option<&str>) -> Utf8PathBuf {
        tedge_flows::flows_dir(config.root_dir(), mapper, profile)
    }

    pub async fn load_flows(
        flows_dir: &Utf8PathBuf,
    ) -> Result<MessageProcessor<BaseFlowRegistry>, Error> {
        let mut processor = MessageProcessor::with_base_registry(flows_dir)
            .await
            .with_context(|| format!("loading flows and steps from {flows_dir}"))?;
        processor.load_all_flows().await;
        Ok(processor)
    }

    pub async fn load_file(
        flows_dir: &Utf8PathBuf,
        path: &Utf8PathBuf,
    ) -> Result<MessageProcessor<BaseFlowRegistry>, Error> {
        let mut processor = MessageProcessor::with_base_registry(flows_dir)
            .await
            .with_context(|| format!("loading flow {path}"))?;

        if let Some("toml") = path.extension() {
            processor.load_single_flow(path).await;
        } else {
            processor.load_single_script(path).await;
        }
        Ok(processor)
    }
}

pub struct ConfigDirCommand {
    pub flows_dir: Utf8PathBuf,
}

#[async_trait::async_trait]
impl Command for ConfigDirCommand {
    fn description(&self) -> String {
        "display flows config directory".to_string()
    }

    async fn execute(&self, _config: TEdgeConfig) -> Result<(), MaybeFancy<Error>> {
        println!("{}", self.flows_dir);
        Ok(())
    }
}
