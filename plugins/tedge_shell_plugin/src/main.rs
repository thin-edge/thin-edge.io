use clap::Parser;
use tedge_config::TEdgeConfig;
use tedge_shell_plugin::bin::ShellCli;
use tedge_shell_plugin::bin::TEdgeConfigView;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = ShellCli::parse();

    let tedge_config = TEdgeConfig::load(&cli.common.config_dir).await?;
    let config = TEdgeConfigView {
        shell: tedge_config.shell.path.to_path_buf(),
        tmp_dir: tedge_config.tmp.path.to_path_buf(),
        max_output_size: tedge_config.shell.max_output_size,
    };

    tedge_shell_plugin::bin::run(cli, config)
}
