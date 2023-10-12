use clap::Parser;

#[tokio::main]
async fn main() {
    let opt = tedge_configuration_plugin::ConfigPluginOpt::parse();
    tedge_configuration_plugin::run(opt).await.unwrap();
}
