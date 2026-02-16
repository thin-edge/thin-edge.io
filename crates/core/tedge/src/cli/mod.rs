pub use self::certificate::*;
use self::refresh_bridges::RefreshBridgesCmd;
use crate::command::BuildCommand;
use crate::command::Command;
use c8y_firmware_plugin::FirmwarePluginOpt;
use c8y_remote_access_plugin::C8yRemoteAccessPluginOpt;
use completions::Shell;
pub use connect::*;
use tedge_agent::AgentOpt;
use tedge_apt_plugin::AptCli;
use tedge_config::cli::CommonArgs;
use tedge_config::TEdgeConfig;
use tedge_file_config_plugin::bin::FileConfigCli;
use tedge_file_log_plugin::bin::FileLogCli;
use tedge_mapper::MapperOpt;
use tedge_watchdog::WatchdogOpt;
use tedge_write::bin::Args as TedgeWriteOpt;

use self::init::TEdgeInitCmd;
mod bridge;
mod certificate;
mod common;
mod completions;
pub mod config;
mod connect;
mod diag;
mod disconnect;
mod flows;
mod http;
mod init;
pub mod log;
mod mqtt;
mod reconnect;
mod refresh_bridges;
mod upload;

#[derive(clap::Parser, Debug)]
#[clap(
    name = clap::crate_name!(),
    version = clap::crate_version!(),
    about = clap::crate_description!(),
    arg_required_else_help(true),
    allow_external_subcommands(true),
    styles(styles()),
    multicall(true),
)]
pub enum TEdgeOptMulticall {
    /// Command line interface to interact with thin-edge.io
    Tedge(TEdgeCli),

    #[clap(flatten)]
    Component(Component),
}

#[derive(clap::Parser, Debug)]
pub struct TEdgeCli {
    #[clap(flatten)]
    pub common: CommonArgs,

    #[clap(subcommand)]
    pub cmd: TEdgeOpt,
}

#[derive(clap::Parser, Debug)]
pub enum Component {
    C8yFirmwarePlugin(FirmwarePluginOpt),

    C8yRemoteAccessPlugin(C8yRemoteAccessPluginOpt),

    TedgeAgent(AgentOpt),

    #[clap(alias = "apt")]
    TedgeAptPlugin(AptCli),

    TedgeFileConfigPlugin(FileConfigCli),

    TedgeFileLogPlugin(FileLogCli),

    TedgeMapper(MapperOpt),

    TedgeWatchdog(WatchdogOpt),

    TedgeWrite(TedgeWriteOpt),
}

#[derive(clap::Parser, Debug)]
#[clap(
    name = clap::crate_name!(),
    version = clap::crate_version!(),
    about = clap::crate_description!(),
)]
pub enum TEdgeOpt {
    /// Initialize Thin Edge
    Init {
        /// The user who will own the directories created
        #[clap(long, default_value = "tedge")]
        user: String,

        /// The group who will own the directories created
        #[clap(long, default_value = "tedge")]
        group: String,

        /// Create symlinks to the tedge binary using a relative path
        /// (e.g. ./tedge) instead of an absolute path (e.g. /usr/bin/tedge)
        #[clap(long)]
        relative_links: bool,
    },

    /// Create and manage device certificate
    #[clap(subcommand)]
    Cert(certificate::TEdgeCertCli),

    /// Configure Thin Edge.
    #[clap(subcommand)]
    Config(config::ConfigCmd),

    /// Connect to cloud provider
    ///
    /// If there is a renewed version of the device certificate,
    /// this new certificate is used to connect the cloud
    /// and on a successful connection the new certificate is promoted as active.
    Connect(connect::TEdgeConnectOpt),

    /// Remove bridge connection for a provider
    Disconnect(disconnect::TEdgeDisconnectBridgeCli),

    /// Collect diagnostic information to help with debugging
    #[clap(subcommand)]
    Diag(diag::TEdgeDiagCli),

    /// Reconnect command, calls disconnect followed by connect
    Reconnect(reconnect::TEdgeReconnectCli),

    /// Refresh all currently active mosquitto bridges
    RefreshBridges,

    /// Upload files to the cloud
    #[clap(subcommand)]
    Upload(upload::UploadCmd),

    /// Publish a message on a topic and subscribe a topic.
    #[clap(subcommand)]
    Mqtt(mqtt::TEdgeMqttCli),

    /// Send HTTP requests to local thin-edge HTTP servers
    #[clap(subcommand)]
    Http(http::TEdgeHttpCli),

    /// Monitor and test flows
    #[clap(subcommand)]
    Flows(flows::TEdgeFlowsCli),

    /// Run thin-edge services and plugins
    Run(ComponentOpt),

    Completions {
        shell: Shell,
    },

    #[clap(subcommand)]
    Bridge(bridge::BridgeCmd),
}

#[derive(Debug, clap::Parser)]
pub struct ComponentOpt {
    #[clap(subcommand)]
    pub component: Component,
}

fn styles() -> clap::builder::Styles {
    clap::builder::Styles::styled()
        .usage(
            anstyle::Style::new()
                .bold()
                .underline()
                .fg_color(Some(anstyle::Color::Ansi(anstyle::AnsiColor::Yellow))),
        )
        .header(
            anstyle::Style::new()
                .bold()
                .underline()
                .fg_color(Some(anstyle::Color::Ansi(anstyle::AnsiColor::Yellow))),
        )
        .literal(
            anstyle::Style::new().fg_color(Some(anstyle::Color::Ansi(anstyle::AnsiColor::Green))),
        )
        .invalid(
            anstyle::Style::new()
                .bold()
                .fg_color(Some(anstyle::Color::Ansi(anstyle::AnsiColor::Red))),
        )
        .error(
            anstyle::Style::new()
                .bold()
                .fg_color(Some(anstyle::Color::Ansi(anstyle::AnsiColor::Red))),
        )
        .valid(
            anstyle::Style::new()
                .bold()
                .underline()
                .fg_color(Some(anstyle::Color::Ansi(anstyle::AnsiColor::Green))),
        )
        .placeholder(
            anstyle::Style::new().fg_color(Some(anstyle::Color::Ansi(anstyle::AnsiColor::White))),
        )
}

#[async_trait::async_trait]
impl BuildCommand for TEdgeOpt {
    async fn build_command(
        self,
        config: &TEdgeConfig,
    ) -> Result<Box<dyn Command>, crate::ConfigError> {
        match self {
            TEdgeOpt::Init {
                user,
                group,
                relative_links,
            } => Ok(Box::new(TEdgeInitCmd::new(user, group, relative_links))),
            TEdgeOpt::Upload(opt) => opt.build_command(config).await,
            TEdgeOpt::Cert(opt) => opt.build_command(config).await,
            TEdgeOpt::Config(opt) => opt.build_command(config).await,
            TEdgeOpt::Connect(opt) => opt.build_command(config).await,
            TEdgeOpt::Diag(opt) => opt.build_command(config).await,
            TEdgeOpt::Disconnect(opt) => opt.build_command(config).await,
            TEdgeOpt::RefreshBridges => RefreshBridgesCmd::new(config).map(Command::into_boxed),
            TEdgeOpt::Mqtt(opt) => opt.build_command(config).await,
            TEdgeOpt::Http(opt) => opt.build_command(config).await,
            TEdgeOpt::Reconnect(opt) => opt.build_command(config).await,
            TEdgeOpt::Flows(opt) => opt.build_command(config).await,
            TEdgeOpt::Bridge(opt) => opt.build_command(config).await,
            TEdgeOpt::Run(_) => {
                // This method has to be kept in sync with tedge::redirect_if_multicall()
                panic!("tedge mapper|agent|write commands are launched as multicall")
            }
            TEdgeOpt::Completions { shell } => shell.build_command(config).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::Component;
    use crate::TEdgeOptMulticall;
    use clap::Parser;

    #[test]
    fn tedge_mapper_rejects_with_missing_argument() {
        assert!(TEdgeOptMulticall::try_parse_from(["tedge-mapper"]).is_err());
    }

    #[test]
    fn tedge_mapper_accepts_with_argument() {
        assert!(matches!(
            TEdgeOptMulticall::parse_from(["tedge-mapper", "c8y"]),
            TEdgeOptMulticall::Component(Component::TedgeMapper(_))
        ));
    }

    #[test]
    fn tedge_agent_runs_with_no_additional_arguments() {
        assert!(matches!(
            TEdgeOptMulticall::parse_from(["tedge-agent"]),
            TEdgeOptMulticall::Component(Component::TedgeAgent(_))
        ));
    }
}
