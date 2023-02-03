use std::process::Output;

use tedge_actors::Actor;
use tedge_actors::ActorBuilder;
use tedge_actors::Builder;
use tedge_actors::ChannelError;
use tedge_actors::ConcurrentServiceActor;
use tedge_actors::RuntimeError;
use tedge_actors::RuntimeHandle;
use tedge_actors::Service;
use tedge_actors::ServiceMessageBoxBuilder;

#[derive(Clone)]
pub struct ScriptActor;

#[derive(Debug)]
pub struct Execute {
    command: String,
    args: Vec<String>,
}

#[async_trait::async_trait]
impl Service for ScriptActor {
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

impl ScriptActorBuilder {
    pub async fn run(self) -> Result<(), ChannelError> {
        self.actor.run(self.box_builder.build()).await
    }
}

pub struct ScriptActorBuilder {
    actor: ConcurrentServiceActor<ScriptActor>,
    box_builder: ServiceMessageBoxBuilder<Execute, std::io::Result<Output>>,
}

#[async_trait::async_trait]
impl ActorBuilder for ScriptActorBuilder {
    async fn spawn(self, runtime: &mut RuntimeHandle) -> Result<(), RuntimeError> {
        let actor = self.actor;
        let messages = self.box_builder.build();
        runtime.run(actor, messages).await
    }
}

#[cfg(test)]
mod tests {
    use tedge_actors::NoConfig;
    use tedge_actors::RequestResponseHandler;

    use super::*;

    #[tokio::test]
    async fn script() {
        let csa = ConcurrentServiceActor::new(ScriptActor);
        let mut builder = ScriptActorBuilder {
            actor: csa,
            box_builder: ServiceMessageBoxBuilder::new("Script", 100),
        };
        let mut handle = RequestResponseHandler::new("Tester", &mut builder.box_builder, NoConfig);

        tokio::spawn(builder.run());

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
