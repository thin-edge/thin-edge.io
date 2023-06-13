pub use shell_words::ParseError;
use std::process::Output;
use tedge_actors::Concurrent;
use tedge_actors::Server;
use tedge_actors::ServerActorBuilder;
use tedge_actors::ServerConfig;

#[derive(Clone)]
pub struct ScriptActor;

#[derive(Debug, Eq, PartialEq)]
pub struct Execute {
    pub command: String,
    pub args: Vec<String>,
}

impl Execute {
    pub fn try_new(command_line: &str) -> Result<Self, ParseError> {
        let mut args = shell_words::split(command_line)?;
        if args.is_empty() {
            Err(ParseError)
        } else {
            let command = args.remove(0);
            Ok(Execute { command, args })
        }
    }
}

#[async_trait::async_trait]
impl Server for ScriptActor {
    type Request = Execute;
    type Response = std::io::Result<Output>;

    fn name(&self) -> &str {
        "Script"
    }

    async fn handle(&mut self, message: Self::Request) -> Self::Response {
        tokio::process::Command::new(message.command)
            .args(message.args)
            .output()
            .await
    }
}

impl ScriptActor {
    pub fn builder() -> ServerActorBuilder<ScriptActor, Concurrent> {
        ServerActorBuilder::new(ScriptActor, &ServerConfig::default(), Concurrent)
    }
}

#[cfg(test)]
mod tests {
    use tedge_actors::ClientMessageBox;

    use super::*;

    #[test]
    fn test_parsing() {
        assert_eq!(
            Execute::try_new(r#"python -c "print('Hello world!')""#),
            Ok(Execute {
                command: "python".to_string(),
                args: vec!["-c".to_string(), "print('Hello world!')".to_string()]
            })
        )
    }

    #[tokio::test]
    async fn script() {
        let mut actor = ScriptActor::builder();
        let mut handle = ClientMessageBox::new("Tester", &mut actor);

        tokio::spawn(actor.run());

        let output = handle
            .await_response(Execute {
                command: "echo".to_owned(),
                args: vec!["A message".to_owned()],
            })
            .await
            .unwrap()
            .unwrap();

        assert!(output.status.success());
        assert_eq!(String::from_utf8(output.stdout).unwrap(), "A message\n");
        assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
    }
}
