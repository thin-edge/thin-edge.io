use serde::de::Error;
use serde::Deserialize;
use serde::Deserializer;
use serde::Serialize;
use serde::Serializer;
use std::fmt::Display;
use std::fmt::Formatter;
use std::str::FromStr;

/// A parsed Unix command line
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShellScript {
    pub command: String,
    pub args: Vec<String>,
}

/// Deserialize an Unix command line
impl<'de> Deserialize<'de> for ShellScript {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let command_line = String::deserialize(deserializer)?;
        command_line.parse().map_err(Error::custom)
    }
}

impl FromStr for ShellScript {
    type Err = String;

    fn from_str(command_line: &str) -> Result<Self, Self::Err> {
        let mut args = shell_words::split(command_line)
            .map_err(|err| format!("invalid script: {command_line}: {err}"))?;
        if args.is_empty() {
            Err("invalid script: empty".to_string())
        } else {
            let script = args.remove(0);
            Ok(ShellScript {
                command: script,
                args,
            })
        }
    }
}

/// Serialize an Unix command line, using appropriate quotes
impl Serialize for ShellScript {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.to_string().serialize(serializer)
    }
}

impl Display for ShellScript {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut args = vec![self.command.clone()];
        args.append(&mut self.args.clone());
        f.write_str(&shell_words::join(args))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_parse_and_display() {
        let script: ShellScript = "sh -c 'sleep 10'".parse().unwrap();
        assert_eq!(
            script,
            ShellScript {
                command: "sh".to_string(),
                args: vec!["-c".to_string(), "sleep 10".to_string()]
            }
        );
        assert_eq!(format!("{script}"), "sh -c 'sleep 10'");
    }
}
