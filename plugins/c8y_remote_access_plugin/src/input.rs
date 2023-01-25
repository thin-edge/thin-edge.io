use miette::ensure;
use miette::miette;
use miette::Context;
use serde::Deserialize;

use crate::csv::deserialize_csv_record;

#[derive(Deserialize, Debug, PartialEq, Eq)]
pub struct RemoteAccessConnect {
    device_id: String,
    host: String,
    port: u16,
    key: String,
}

pub fn parse_arguments() -> miette::Result<RemoteAccessConnect> {
    let arg_csv = std::env::args()
        .nth(1)
        .ok_or_else(|| miette!("Expected one argument to the remote access plugin process"))?;

    RemoteAccessConnect::deserialize_smartrest(&arg_csv)
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
    use super::*;

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
