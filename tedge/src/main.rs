use structopt::StructOpt;

mod cli;

fn main() {
    let cmd = cli::TEdgeCmd::from_args();
    match cmd {
        cli::TEdgeCmd::Mqtt(mqtt) => mqtt.exec().unwrap(), // unwrap is not good, but don't know whatelse I can use.
        _ => println!("{:#?}", cmd),
    }
}
