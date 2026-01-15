use clap::Parser;
use tedge_config::TEdgeConfig;
use tedge_file_config_plugin::bin::FileConfigCli;
use tedge_file_config_plugin::bin::TEdgeConfigView;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = FileConfigCli::parse();

    // Load tedge config to determine if sudo is enabled
    let tedge_config = TEdgeConfig::load(&cli.common.config_dir).await?;
    let tedge_config_view = TEdgeConfigView::new(tedge_config.sudo.enable);

    tedge_file_config_plugin::bin::run(cli, tedge_config_view).await
}
