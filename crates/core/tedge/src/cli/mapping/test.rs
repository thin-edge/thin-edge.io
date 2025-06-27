use crate::cli::mapping::TEdgeMappingCli;
use crate::command::Command;
use crate::log::MaybeFancy;
use anyhow::Error;
use std::path::PathBuf;
use tedge_config::TEdgeConfig;
use tedge_gen_mapper::pipeline::*;
use tedge_gen_mapper::MessageProcessor;
use tokio::io::AsyncBufReadExt;
use tokio::io::BufReader;
use tokio::io::Stdin;

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
            self.process(&mut processor, message, &timestamp).await;
        } else {
            let mut stdin = BufReader::new(tokio::io::stdin());
            while let Some(message) = next_message(&mut stdin).await {
                let timestamp = DateTime::now();
                self.process(&mut processor, &message, &timestamp).await;
            }
        }
        Ok(())
    }
}

impl TestCommand {
    async fn process(
        &self,
        processor: &mut MessageProcessor,
        message: &Message,
        timestamp: &DateTime,
    ) {
        match &self.filter {
            Some(filter) => {
                let filter_name = filter.display().to_string();
                print(
                    processor
                        .process_with_pipeline(&filter_name, timestamp, message)
                        .await,
                )
            }
            None => processor
                .process(timestamp, message)
                .await
                .into_iter()
                .map(|(_, v)| v)
                .for_each(print),
        }
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

fn parse(line: String) -> Result<Option<Message>, Error> {
    let line = line.trim();
    if line.is_empty() {
        return Ok(None);
    }
    if !line.starts_with("[") {
        return Err(anyhow::anyhow!("Missing opening bracket: {}", line));
    }
    let Some(closing_bracket) = line.find(']') else {
        return Err(anyhow::anyhow!("Missing closing bracket: {}", line));
    };

    let topic = line[1..closing_bracket].to_string();
    let payload = line[closing_bracket + 1..].to_string();

    Ok(Some(Message { topic, payload }))
}

async fn next_message(input: &mut BufReader<Stdin>) -> Option<Message> {
    let mut line = String::new();
    match input.read_line(&mut line).await {
        Ok(0) => None,
        Ok(_) => match parse(line) {
            Ok(message) => message,
            Err(err) => {
                eprintln!("Fail to parse input message {}", err);
                None
            }
        },
        Err(err) => {
            eprintln!("Fail to read input stream {}", err);
            None
        }
    }
}
