use clap::Parser;
use tedge_config::TEdgeConfig;
use tedge_shell_plugin::bin::ShellCli;
use tedge_shell_plugin::bin::TEdgeConfigView;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = ShellCli::parse();

    let tedge_config = TEdgeConfig::load(&cli.common.config_dir).await?;
    let config = TEdgeConfigView::new(&tedge_config);

    tedge_shell_plugin::bin::run(cli, config).await
}
