use std::path::PathBuf;

pub use self::certificate::*;
use self::refresh_bridges::RefreshBridgesCmd;
use crate::command::BuildCommand;
use crate::command::BuildContext;
use crate::command::Command;
use c8y_firmware_plugin::FirmwarePluginOpt;
use c8y_remote_access_plugin::C8yRemoteAccessPluginOpt;
pub use connect::*;
use tedge_agent::AgentOpt;
use tedge_config::get_config_dir;
use tedge_mapper::MapperOpt;
use tedge_watchdog::WatchdogOpt;
use tedge_write::bin::Args as TedgeWriteOpt;

use self::init::TEdgeInitCmd;
mod certificate;
mod common;
pub mod config;
mod connect;
mod disconnect;
mod init;
pub mod log;
mod mqtt;
mod reconnect;
mod refresh_bridges;

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
    Tedge {
        #[clap(subcommand)]
        cmd: TEdgeOpt,

        /// [env: TEDGE_CONFIG_DIR, default: /etc/tedge]
        #[clap(
            long = "config-dir",
            default_value = get_config_dir().into_os_string(),
            hide_env_values = true,
            hide_default_value = true,
            global = true,
        )]
        config_dir: PathBuf,
    },

    #[clap(flatten)]
    Component(Component),
}

#[derive(clap::Parser, Debug)]
pub enum Component {
    TedgeMapper(MapperOpt),

    TedgeAgent(AgentOpt),

    C8yFirmwarePlugin(FirmwarePluginOpt),

    TedgeWatchdog(WatchdogOpt),

    C8yRemoteAccessPlugin(C8yRemoteAccessPluginOpt),

    TedgeWrite(TedgeWriteOpt),
}

#[derive(clap::Subcommand, Debug)]
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

    /// Connect to connector provider
    #[clap(subcommand)]
    Connect(connect::TEdgeConnectOpt),

    /// Remove bridge connection for a provider
    #[clap(subcommand)]
    Disconnect(disconnect::TEdgeDisconnectBridgeCli),

    /// Reconnect command, calls disconnect followed by connect
    #[clap(subcommand)]
    Reconnect(reconnect::TEdgeReconnectCli),

    /// Refresh all currently active mosquitto bridges
    RefreshBridges,

    /// Publish a message on a topic and subscribe a topic.
    #[clap(subcommand)]
    Mqtt(mqtt::TEdgeMqttCli),
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

impl BuildCommand for TEdgeOpt {
    fn build_command(self, context: BuildContext) -> Result<Box<dyn Command>, crate::ConfigError> {
        match self {
            TEdgeOpt::Init {
                user,
                group,
                relative_links,
            } => Ok(Box::new(TEdgeInitCmd::new(
                user,
                group,
                relative_links,
                context,
            ))),
            TEdgeOpt::Cert(opt) => opt.build_command(context),
            TEdgeOpt::Config(opt) => opt.build_command(context),
            TEdgeOpt::Connect(opt) => opt.build_command(context),
            TEdgeOpt::Disconnect(opt) => opt.build_command(context),
            TEdgeOpt::RefreshBridges => RefreshBridgesCmd::new(&context).map(Command::into_boxed),
            TEdgeOpt::Mqtt(opt) => opt.build_command(context),
            TEdgeOpt::Reconnect(opt) => opt.build_command(context),
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
