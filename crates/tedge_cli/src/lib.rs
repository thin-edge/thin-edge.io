
#[derive(StructOpt, Debug)]
#[structopt(
    name = clap::crate_name!(),
    version = clap::crate_version!(),
    about = clap::crate_description!()
)]
pub struct TEdgeCli {
    #[structopt(subcommand)]
    tedge_command: TEdgeCommandCli,
}

#[derive(StructOpt, Debug)]
enum TEdgeCommandCli {
    /// Create and manage device certificate
    Cert(CertCommandCli),

    /// Configure Thin Edge.
    Config(ConfigCommandCli),

    /// Connect to connector provider
    Connect(ConnectCommandCli),

    /// Remove bridge connection for a provider
    Disconnect(DisconnectCommandCli),

    /// Publish a message on a topic and subscribe a topic.
    Mqtt(MqttCommandCli),
}

#[derive(StructOpt, Debug)]
enum CertCommandCli {
    // ...
}
 
#[derive(StructOpt, Debug)]
enum ConfigCommandCli {
    /// Get the value of the provided configuration key
    Get {
        /// Configuration key. Run `tedge config list --doc` for available keys
        key: tedge_command_config::ConfigKey,
    },

    /// Set or update the provided configuration key with the given value
    Set {
        /// Configuration key. Run `tedge config list --doc` for available keys
        key: tedge_command_config::ConfigKey,

        /// Configuration value.
        value: String,
    },

    /// Unset the provided configuration key
    Unset {
        /// Configuration key. Run `tedge config list --doc` for available keys
        key: tedge_command_config::ConfigKey,
    },

    /// Print the configuration keys and their values
    List {
        /// Prints all the configuration keys, even those without a configured value
        #[structopt(long = "all")]
        is_all: bool,

        /// Prints all keys and descriptions with example values
        #[structopt(long = "doc")]
        is_doc: bool,
    },
}

#[derive(StructOpt, Debug)]
enum ConnectCommandCli {
    // ...
}

#[derive(StructOpt, Debug)]
enum DisconnectCommandCli {
    // ...
}


// This would ideally live in `tedge_command_tedge`.
pub enum TEdgeCommand {
    CertCommand(tedge_command_cert::CertCommand),
    GetConfigCommand(tedge_command_config::GetConfigCommand),
    ConnectCommand(tedge_command_connect::ConnectCommand),
    // ...
}

impl Command for TEdgeCommand {
    type Error = anyhow::Error; // .. either the top-level error type that accumulates all the sub-command error types, or just anyhow::Error

    fn description(&self) -> String {
        match self {
            Self::CertCommand(cmd) => cmd.description(),
            Self::GetConfigCommand(cmd) => cmd.description(),
            Self::ConnectCommand(cmd) => cmd.description(),
        }
    }

    fn execute(self) -> Result<(), Self::Error> {
        match self {
            Self::CertCommand(cmd) => cmd.execute(),
            Self::GetConfigCommand(cmd) => cmd.execute(),
            Self::ConnectCommand(cmd) => cmd.execute(),
        }
    }
}
 
impl TEdgeCli {
    pub fn into_command(self, context: ...) -> Result<TEdgeCommand, CliError> {
        match self.tedge_command {
            TEdgeCommandCli::Cert(opt) => {
                unimplemented!()
                // ...
            }
            TEdgeCommandCli::Config(config_cmd) => {
                match config_cmd {
                    ConfigCommandCli::Get { key } => Ok(
                        TEdgeCommand::GetConfigCommand(
                        tedge_command_config::GetConfigCommand {
                            config_key: key,
                            config
                        }))

                    // ...
                }
            }
            TEdgeOpt::Connect(opt) => {
                unimplemented!()
            }
            TEdgeOpt::Disconnect(opt) => {
                unimplemented!()
            }
            TEdgeOpt::Mqtt(opt) => {
                unimplemented!()
            }
        }

    }
}

// These kind of tests ensure that our CLI parser works correctly.
#[test]
fn test_cli_parsing() {
    let cli: TEdgeCli = TEdgeCli::parse_from("tedge config set device.id 123")?; 
    assert_matches!(cli, TEdgeCli {
        tedge_command: TEdgeCommandCli::Config(
                           ConfigCommandCli::Set {
                               key: tedge_command_config::ConfigKey::from_str("device.id")?,
                               value: "123",
                           }
        )
    });
}

// These kind of tests ensure that given a TEdgeCli, we convert it into the correct commands.
#[test]
fn test_cli_into_command() {
    let cli = TEdgeCli {
        tedge_command: TEdgeCommandCli::Config(
                           ConfigCommandCli::Set {
                               key: tedge_command_config::ConfigKey::from_str("device.id")?,
                               value: "123".into(),
                           }
        )
    };

    let command = cli.into_command();

    assert_matches!(command, TEdgeCommand::SetConfigCommand {
      key: tedge_command_config::ConfigKey::from_str("device.id")?,
      value: "123".into(),
      // ...
    });
    assert_matches!(command.description(), "....");
}

#[test]
fn test_command_execution() {
    // XXX: We do not run commands here! We should have unit tests in each `tedge_command_*` crate
    // that test that the command works properly.
}
