use crate::cli::mapping::TEdgeMappingCli;
use crate::command::Command;
use crate::log::MaybeFancy;
use anyhow::Error;
use std::path::PathBuf;
use tedge_config::TEdgeConfig;
use tedge_gen_mapper::pipeline::Pipeline;

pub struct ListCommand {
    pub mapping_dir: PathBuf,
    pub topic: Option<String>,
}

#[async_trait::async_trait]
impl Command for ListCommand {
    fn description(&self) -> String {
        format!(
            "list pipelines and filters in {:}",
            self.mapping_dir.display()
        )
    }

    async fn execute(&self, _config: TEdgeConfig) -> Result<(), MaybeFancy<Error>> {
        let processor = TEdgeMappingCli::load_pipelines(&self.mapping_dir).await?;

        match &self.topic {
            Some(topic) => processor
                .pipelines
                .iter()
                .filter(|(_, pipeline)| pipeline.topics().accept_topic_name(topic))
                .for_each(Self::display),

            None => processor.pipelines.iter().for_each(Self::display),
        }

        Ok(())
    }
}

impl ListCommand {
    fn display((pipeline_id, pipeline): (&String, &Pipeline)) {
        println!("{pipeline_id}");
        for stage in pipeline.stages.iter() {
            println!("\t{}", stage.filter.path.display());
        }
    }
}
