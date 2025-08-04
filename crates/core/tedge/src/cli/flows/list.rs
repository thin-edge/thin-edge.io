use crate::cli::flows::TEdgeFlowsCli;
use crate::command::Command;
use crate::log::MaybeFancy;
use anyhow::Error;
use std::path::PathBuf;
use tedge_config::TEdgeConfig;
use tedge_gen_mapper::flow::Flow;

pub struct ListCommand {
    pub flows_dir: PathBuf,
    pub topic: Option<String>,
}

#[async_trait::async_trait]
impl Command for ListCommand {
    fn description(&self) -> String {
        format!("list flows and flow steps in {:}", self.flows_dir.display())
    }

    async fn execute(&self, _config: TEdgeConfig) -> Result<(), MaybeFancy<Error>> {
        let processor = TEdgeFlowsCli::load_flows(&self.flows_dir).await?;

        match &self.topic {
            Some(topic) => processor
                .flows
                .iter()
                .filter(|(_, flow)| flow.topics().accept_topic_name(topic))
                .for_each(Self::display),

            None => processor.flows.iter().for_each(Self::display),
        }

        Ok(())
    }
}

impl ListCommand {
    fn display((flow_id, flow): (&String, &Flow)) {
        println!("{flow_id}");
        for step in flow.steps.iter() {
            println!("\t{}", step.script.path.display());
        }
    }
}
