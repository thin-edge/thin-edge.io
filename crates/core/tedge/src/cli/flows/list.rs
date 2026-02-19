use crate::cli::flows::TEdgeFlowsCli;
use crate::command::Command;
use crate::log::MaybeFancy;
use anyhow::Error;
use camino::Utf8PathBuf;
use tedge_config::TEdgeConfig;
use tedge_flows::Flow;
use tedge_flows::FlowRegistryExt;

pub struct ListCommand {
    pub flows_dir: Utf8PathBuf,
    pub topic: Option<String>,
}

#[async_trait::async_trait]
impl Command for ListCommand {
    fn description(&self) -> String {
        format!("list flows and flow steps in {}", self.flows_dir)
    }

    async fn execute(&self, _config: TEdgeConfig) -> Result<(), MaybeFancy<Error>> {
        let processor = TEdgeFlowsCli::load_flows(&self.flows_dir).await?;

        match &self.topic {
            Some(topic) => processor
                .registry
                .flows()
                .filter(|flow| flow.topics().accept_topic_name(topic))
                .for_each(Self::display),

            None => processor.registry.flows().for_each(Self::display),
        }

        Ok(())
    }
}

impl ListCommand {
    fn display(flow: &Flow) {
        let name = flow.name();
        let version = flow
            .version
            .as_ref()
            .map(|v| format!("v{v}"))
            .unwrap_or_else(|| "unversioned".to_string());
        let location = flow.source.to_string();

        println!("Flow        : {name} ({version})");
        println!(
            "Description : {}",
            flow.description
                .as_ref()
                .map(|v| v.to_string())
                .unwrap_or_else(|| "none".to_string())
        );
        println!("File        : {location}");

        if let Some(tags) = &flow.tags {
            println!("Tags        : [{}]", tags.join(","));
        }

        if !flow.steps.is_empty() {
            for (i, step) in flow.steps.iter().enumerate() {
                println!("Step {:?}      : {}", i + 1, step.source());
            }
        }
        println!(); // Add a blank line for separation
    }
}
