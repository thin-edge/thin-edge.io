use crate::cli::flows::TEdgeFlowsCli;
use crate::command::Command;
use crate::log::MaybeFancy;
use anyhow::Error;
use base64::prelude::BASE64_STANDARD;
use base64::prelude::*;
use camino::Utf8PathBuf;
use tedge_config::TEdgeConfig;
use tedge_flows::flow::*;
use tedge_flows::MessageProcessor;
use tokio::io::AsyncBufReadExt;
use tokio::io::BufReader;
use tokio::io::Stdin;
use tokio::time::Instant;

pub struct TestCommand {
    pub flows_dir: Utf8PathBuf,
    pub flow: Option<Utf8PathBuf>,
    pub message: Option<Message>,
    pub final_on_interval: bool,
    pub base64_input: bool,
    pub base64_output: bool,
}

#[async_trait::async_trait]
impl Command for TestCommand {
    fn description(&self) -> String {
        format!(
            "process message samples using flows and steps in {}",
            self.flows_dir
        )
    }

    async fn execute(&self, _config: TEdgeConfig) -> Result<(), MaybeFancy<Error>> {
        let mut processor = match &self.flow {
            None => TEdgeFlowsCli::load_flows(&self.flows_dir).await?,
            Some(flow) => TEdgeFlowsCli::load_file(&self.flows_dir, flow).await?,
        };
        if let Some(message) = &self.message {
            let timestamp = DateTime::now();
            self.process(&mut processor, message.clone(), timestamp)
                .await;
        } else {
            let mut stdin = BufReader::new(tokio::io::stdin());
            while let Some(message) = next_message(&mut stdin).await {
                let timestamp = DateTime::now();
                self.process(&mut processor, message, timestamp).await;
            }
        }
        if self.final_on_interval {
            let timestamp = DateTime::now();
            let now = processor
                .last_interval_deadline()
                .unwrap_or_else(Instant::now);
            self.tick(&mut processor, timestamp, now).await;
        }
        Ok(())
    }
}

impl TestCommand {
    async fn process(
        &self,
        processor: &mut MessageProcessor,
        mut message: Message,
        timestamp: DateTime,
    ) {
        if self.base64_input {
            match BASE64_STANDARD.decode(message.payload) {
                Ok(decoded) => message.payload = decoded,
                Err(err) => {
                    tracing::error!("Cannot decode message: {}", err);
                    return;
                }
            }
        };
        let source = SourceTag::Mqtt;
        processor
            .on_message(timestamp, &source, &message)
            .await
            .into_iter()
            .for_each(|msg| self.print_messages(msg))
    }

    async fn tick(&self, processor: &mut MessageProcessor, timestamp: DateTime, now: Instant) {
        processor
            .on_interval(timestamp, now)
            .await
            .into_iter()
            .for_each(|msg| self.print_messages(msg))
    }

    fn print_messages(&self, result: FlowResult) {
        match result {
            FlowResult::Ok { mut messages, .. } => {
                if self.base64_output {
                    for message in messages.iter_mut() {
                        message.payload = BASE64_STANDARD.encode(&message.payload).into_bytes();
                    }
                }
                for message in messages {
                    println!("{}", message);
                }
            }
            FlowResult::Err { flow, error, .. } => {
                tracing::error!("Error in {flow}: {}", error)
            }
        }
    }
}
fn parse_input(line: String) -> Result<Option<Message>, Error> {
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
    let message = Message::new(topic, payload);

    Ok(Some(message))
}

async fn next_line(input: &mut BufReader<Stdin>) -> Option<String> {
    loop {
        let mut line = String::new();
        match input.read_line(&mut line).await {
            Ok(0) => return None,
            Ok(_) => {
                let line = line.trim();
                if !line.is_empty() {
                    return Some(line.to_string());
                }
            }
            Err(err) => {
                eprintln!("Fail to read input stream {}", err);
                return None;
            }
        }
    }
}

async fn next_message(input: &mut BufReader<Stdin>) -> Option<Message> {
    let line = next_line(input).await?;
    match parse_input(line) {
        Ok(message) => message,
        Err(err) => {
            eprintln!("Fail to parse input message {}", err);
            None
        }
    }
}
