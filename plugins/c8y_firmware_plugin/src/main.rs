use clap::Parser;

#[tokio::main]
async fn main() {
    let opt = c8y_firmware_plugin::FirmwarePluginOpt::parse();
    c8y_firmware_plugin::run(opt).await.unwrap();
}
