use c8y_configuration_plugin::ConfigPluginOpt;
use clap::Parser;

#[tokio::main]
async fn main() {
    let opt = ConfigPluginOpt::parse();
    c8y_configuration_plugin::run(opt).await.unwrap();
}
