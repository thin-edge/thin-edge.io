use clap::Parser;

#[tokio::main]
async fn main() {
    let opt = tedge_log_plugin::LogfilePluginOpt::parse();
    tedge_log_plugin::run(opt).await.unwrap();
}
