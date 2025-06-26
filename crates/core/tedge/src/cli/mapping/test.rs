use crate::cli::mapping::TEdgeMappingCli;
use crate::command::Command;
use crate::log::MaybeFancy;
use anyhow::Error;
use std::path::PathBuf;
use tedge_config::TEdgeConfig;
use tedge_gen_mapper::pipeline::*;

pub struct TestCommand {
    pub mapping_dir: PathBuf,
    pub filter: Option<PathBuf>,
    pub message: Option<Message>,
}

#[async_trait::async_trait]
impl Command for TestCommand {
    fn description(&self) -> String {
        format!(
            "process message samples using pipelines and filters in {:}",
            self.mapping_dir.display()
        )
    }

    async fn execute(&self, _config: TEdgeConfig) -> Result<(), MaybeFancy<Error>> {
        let mut processor = TEdgeMappingCli::load_pipelines(&self.mapping_dir).await?;
        if let Some(message) = &self.message {
            let timestamp = DateTime::now();
            match &self.filter {
                Some(filter) => {
                    let filter_name = filter.display().to_string();
                    print(
                        processor
                            .process_with_pipeline(&filter_name, &timestamp, message)
                            .await,
                    )
                }
                None => processor
                    .process(&timestamp, message)
                    .await
                    .into_iter()
                    .map(|(_, v)| v)
                    .for_each(print),
            }
        }

        Ok(())
    }
}

fn print(messages: Result<Vec<Message>, FilterError>) {
    match messages {
        Ok(messages) => {
            for message in messages {
                println!("[{}] {}", message.topic, message.payload);
            }
        }
        Err(err) => {
            eprintln!("Error: {}", err)
        }
    }
}
