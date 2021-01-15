use anyhow::Context;
use structopt::StructOpt;

mod cli;
mod command;
mod mqtt;

fn main() -> anyhow::Result<()> {
    let opt = cli::Opt::from_args();
    opt.run()
        .with_context(|| format!("fail to {}", opt.to_string()))
}

// #[tokio::main]
// async fn main() {
//     let cmd = cli::TEdgeCmd::from_args();
//     match cmd {
//         cli::TEdgeCmd::Mqtt(mqtt) => mqtt.exec().await.expect("tedge mqtt error"),
//         _ => println!("{:#?}", cmd),
//     }
// }
