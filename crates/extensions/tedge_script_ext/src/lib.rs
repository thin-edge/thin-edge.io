use std::process::Output;
use tedge_actors::Concurrent;
use tedge_actors::Server;
use tedge_actors::ServerActorBuilder;
use tedge_actors::ServerConfig;

#[derive(Clone)]
pub struct ScriptActor;

#[derive(Debug)]
pub struct Execute {
    command: String,
    args: Vec<String>,
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
