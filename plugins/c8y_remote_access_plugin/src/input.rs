use clap::ArgGroup;
use clap::Parser;
use miette::ensure;
use miette::miette;
use miette::Context;
use serde::Deserialize;

use crate::csv::deserialize_csv_record;

#[derive(Parser, Deserialize, Debug, PartialEq, Eq)]
pub struct RemoteAccessConnect {
    device_id: String,
    host: String,
    port: u16,
    key: String,
}

#[derive(Parser)]
#[clap(group(ArgGroup::new("install").args(&["init", "cleanup", "connect_string", "child"])))]
#[clap(
name = clap::crate_name!(),
version = clap::crate_version!(),
about = clap::crate_description!(),
)]
struct Cli {
    #[arg(long)]
    /// Complete the installation of c8y-configuration-plugin by declaring the supported operation.
    init: bool,

    #[arg(long)]
    /// Clean up c8y-configuration-plugin, deleting the supported operation from tedge.
    cleanup: bool,

    /// The SmartREST connect message, forwarded from mosquitto by tedge-mapper.
    ///
    /// Can only be provided when neither '--init' nor '--cleanup' are provided.
    connect_string: Option<String>,

    #[arg(long)]
    child: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Init,
    Cleanup,
    SpawnChild(String),
    Connect(RemoteAccessConnect),
}

pub fn parse_arguments() -> miette::Result<Command> {
    Cli::parse().try_into()
}

impl TryFrom<Cli> for Command {
    type Error = miette::Error;
    fn try_from(arguments: Cli) -> Result<Self, Self::Error> {
        match arguments {
            Cli { init: true, .. } => Ok(Command::Init),
            Cli { cleanup: true, .. } => Ok(Command::Cleanup),
            Cli {
                connect_string: Some(message),
                ..
            } => Ok(Command::SpawnChild(message)),
            Cli {
                child: Some(message),
                ..
            } => RemoteAccessConnect::deserialize_smartrest(&message).map(Command::Connect),
            _ => Err(miette!(
                "Expected one argument to the remote access plugin process"
            )),
        }
    }
}

impl RemoteAccessConnect {
    fn deserialize_smartrest(message: &str) -> miette::Result<Self> {
        let (id, command): (u16, Self) = deserialize_csv_record(message)
            .context("Deserialising arguments of remote access connect message")?;
        ensure!(
            id == 530,
            "SmartREST message is not a RemoteAccessConnect operation"
        );
        Ok(command)
    }

    pub fn target_address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    pub fn key(&self) -> &str {
        &self.key
    }
}

#[cfg(test)]
mod tests {
    use std::iter;

    use super::*;
    use miette::IntoDiagnostic;
    use rstest::*;

    #[rstest]
    #[case::init_and_cleanup(&["--init", "--cleanup"])]
    #[case::init_and_command_string(&["--init", "530,jrh-rc-test0,127.0.0.1,22,cd8fc847-f4f2-4712-8dd7-31496aef0a7d"])]
    #[case::cleanup_and_command_string(&["--cleanup", "530,jrh-rc-test0,127.0.0.1,22,cd8fc847-f4f2-4712-8dd7-31496aef0a7d"])]
    #[case::cleanup_and_child_string(&["--cleanup", "--child", "530,jrh-rc-test0,127.0.0.1,22,cd8fc847-f4f2-4712-8dd7-31496aef0a7d"])]
    fn arguments_are_mutually_exclusive(#[case] arguments: &[&str]) {
        try_parse_arguments(arguments).unwrap_err();
    }

    #[rstest]
    #[case::init("--init", Command::Init)]
    #[case::cleanup("--cleanup", Command::Cleanup)]
    fn parses_lifecycle_flags(#[case] argument: &str, #[case] expected: Command) {
        assert_eq!(try_parse_arguments(&[argument]).unwrap(), expected);
    }

    #[test]
    fn parses_spawn_child_if_no_flag_is_provided() {
        let input = "530,jrh-rc-test0,127.0.0.1,22,cd8fc847-f4f2-4712-8dd7-31496aef0a7d";

        let command = try_parse_arguments(&[input]).unwrap();

        assert_eq!(command, Command::SpawnChild(input.to_owned()))
    }

    #[test]
    fn parses_command_string_if_child_flag_is_provided() {
        let input = "530,jrh-rc-test0,127.0.0.1,22,cd8fc847-f4f2-4712-8dd7-31496aef0a7d";

        let command = try_parse_arguments(&["--child", input]).unwrap();

        assert!(matches!(command, Command::Connect(_)))
    }

    fn try_parse_arguments(arguments: &[&str]) -> miette::Result<Command> {
        Cli::try_parse_from(iter::once(&"c8y-remote-access-plugin").chain(arguments))
            .into_diagnostic()?
            .try_into()
    }

    #[test]
    fn parses_command_from_a_530_message() {
        let input = "530,jrh-rc-test0,127.0.0.1,22,cd8fc847-f4f2-4712-8dd7-31496aef0a7d";
        let expected = RemoteAccessConnect {
            device_id: "jrh-rc-test0".into(),
            host: "127.0.0.1".into(),
            port: 22,
            key: "cd8fc847-f4f2-4712-8dd7-31496aef0a7d".into(),
        };

        assert_eq!(
            RemoteAccessConnect::deserialize_smartrest(input).unwrap(),
            expected
        );
    }

    #[test]
    fn rejects_input_if_it_is_not_a_530_message() {
        let input = "71,abcdef";

        RemoteAccessConnect::deserialize_smartrest(input).unwrap_err();
    }

    #[test]
    fn generates_the_target_address_by_combining_the_specified_host_and_port() {
        let input = "530,jrh-rc-test0,127.0.0.1,22,cd8fc847-f4f2-4712-8dd7-31496aef0a7d";

        let command = RemoteAccessConnect::deserialize_smartrest(input).unwrap();

        assert_eq!(command.target_address(), "127.0.0.1:22");
    }
}
