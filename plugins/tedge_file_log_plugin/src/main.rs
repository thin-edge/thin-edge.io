use clap::Parser;
use tedge_config::TEdgeConfig;
use tedge_file_log_plugin::bin::FileLogCli;
use tedge_file_log_plugin::bin::TEdgeConfigView;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = FileLogCli::parse();

    let tedge_config = TEdgeConfig::load(&cli.common.config_dir).await?;
    let tmp_dir = tedge_config.tmp.path.as_path();

    let view = TEdgeConfigView::new(tmp_dir);

    tedge_file_log_plugin::bin::run(cli, view)
}
