use clap::Parser;

#[tokio::main]
async fn main() {
    let opt = tedge_watchdog::WatchdogOpt::parse();
    tedge_watchdog::run(opt).await.unwrap();
}
