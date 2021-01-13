use mqtt_client::{Config, Message, Topic};
use structopt::StructOpt;
mod cli;

#[tokio::main]
async fn main() {
    let cmd = cli::TEdgeCmd::from_args();
    match cmd {
        cli::TEdgeCmd::Mqtt(mqtt) => mqtt.exec().await.expect("tedge mqtt error"),
        _ => println!("{:#?}", cmd),
    }
}
