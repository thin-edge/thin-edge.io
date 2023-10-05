use c8y_remote_access_plugin::C8yRemoteAccessPluginOpt;
use clap::Parser;

#[tokio::main]
async fn main() {
    miette::set_panic_hook();
    let opt = C8yRemoteAccessPluginOpt::parse();
    c8y_remote_access_plugin::run(opt).await.unwrap();
}
